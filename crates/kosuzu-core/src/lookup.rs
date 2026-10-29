//! 词条查表引擎。
//!
//! 设计约束（与原 C++ 版本一致，且更重要）：
//! * 绘制回调里**不加锁、不分配**。表用 `ArcSwap` 做无锁发布，读者只做一次原子读 + 引用计数；
//! * 换表时不阻塞在绘制线程上，旧快照由 epoch 回收，不在回调里释放；
//! * 查表用自定义哈希器（FxHash 变体），因为键是短 UTF-16 串，SipHash 在这里是纯浪费。

use std::collections::HashMap;
use std::hash::{BuildHasherDefault, Hasher};
use std::sync::atomic::{AtomicU32, Ordering};

use arc_swap::ArcSwapOption;

use crate::dict::{self, Rule, MAX_RULE_UNITS, MAX_RULES};

/// FxHash 变体：短键查表比默认 SipHash 快一个数量级。
#[derive(Default, Clone, Copy)]
pub struct FxHasher {
    hash: u64,
}

const FX_SEED: u64 = 0x51_7c_c1_b7_27_22_0a_95;

impl FxHasher {
    #[inline]
    fn add(&mut self, word: u64) {
        self.hash = (self.hash.rotate_left(5) ^ word).wrapping_mul(FX_SEED);
    }
}

impl Hasher for FxHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.hash
    }

    #[inline]
    fn write(&mut self, bytes: &[u8]) {
        let mut chunks = bytes.chunks_exact(8);
        for chunk in &mut chunks {
            let mut word = [0u8; 8];
            word.copy_from_slice(chunk);
            self.add(u64::from_le_bytes(word));
        }
        let rest = chunks.remainder();
        if !rest.is_empty() {
            let mut word = [0u8; 8];
            word[..rest.len()].copy_from_slice(rest);
            self.add(u64::from_le_bytes(word));
        }
    }

    #[inline]
    fn write_u16(&mut self, value: u16) {
        self.add(value as u64);
    }

    #[inline]
    fn write_u32(&mut self, value: u32) {
        self.add(value as u64);
    }

    #[inline]
    fn write_u64(&mut self, value: u64) {
        self.add(value);
    }

    #[inline]
    fn write_usize(&mut self, value: usize) {
        self.add(value as u64);
    }
}

/// 同一条原文可能同时被登记为「名称」和「参数」，两者译文不同时必须放弃替换。
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Entry {
    pub name: Option<Vec<u16>>,
    pub param: Option<Vec<u16>>,
}

pub type Table = HashMap<Vec<u16>, Entry, BuildHasherDefault<FxHasher>>;

/// 当前生效的只读快照。`None` 表示尚未装载词典。
static TABLE: ArcSwapOption<Table> = ArcSwapOption::const_empty();
/// 位 0 = 名称，位 1 = 参数。
static MODES: AtomicU32 = AtomicU32::new(0);

/// 翻译结果码，与原 `LB_Translate` 完全一致。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
pub enum Translate {
    Original = 0,
    Translated = 1,
    Ambiguous = 2,
    BufferTooSmall = 3,
    Invalid = -1,
}

/// 装载规则。任何一条不合法都整体失败，旧快照保持不变（对应「无效词典重载保留旧快照」）。
pub fn set_rules(rules: &[Rule]) -> Result<(), ()> {
    set_rules_owned(rules.to_vec())
}

/// 同 [`set_rules`]，但直接吃掉所有权，避免装载 17k 条时多一次整体拷贝。
pub fn set_rules_owned(rules: Vec<Rule>) -> Result<(), ()> {
    if rules.len() > MAX_RULES as usize {
        return Err(());
    }
    let mut table: Table =
        Table::with_capacity_and_hasher(rules.len(), BuildHasherDefault::default());
    for rule in rules {
        if !dict::validate_rule(&rule) {
            return Err(());
        }
        let entry = table.entry(rule.source).or_default();
        match rule.category {
            dict::CATEGORY_NAME => entry.name = Some(rule.target),
            _ => entry.param = Some(rule.target),
        }
    }
    TABLE.store(Some(std::sync::Arc::new(table)));
    Ok(())
}

/// 清空规则（主要给测试用）。
pub fn clear_rules() {
    TABLE.store(None);
}

pub fn set_modes(value: u32) {
    MODES.store(value & 3, Ordering::Release);
}

pub fn modes() -> u32 {
    MODES.load(Ordering::Acquire)
}

pub fn rule_count() -> usize {
    TABLE.load().as_ref().map_or(0, |table| table.len())
}

/// 无锁、无分配的查表。
///
/// # Safety
/// `source` 必须指向 `length` 个可读的 `u16`；`output` 必须指向 `capacity` 个可写的 `u16`。
/// 两者都允许为未初始化内存（只写入，不读取），这是绘制回调里省掉 16 KiB memset 的关键。
#[inline]
pub unsafe fn translate_raw(
    source: *const u16,
    length: u32,
    output: *mut u16,
    capacity: u32,
) -> i32 {
    let enabled = MODES.load(Ordering::Acquire);
    if enabled == 0 {
        return Translate::Original as i32;
    }
    if source.is_null() || output.is_null() || length > MAX_RULE_UNITS {
        return Translate::Invalid as i32;
    }
    if output as *const u16 == source {
        return Translate::Invalid as i32;
    }

    // `load()` 走 hazard 快路径：不增引用计数，绘制回调里更省。
    let snapshot = TABLE.load();
    let table = match snapshot.as_ref() {
        Some(table) => table.as_ref(),
        None => return Translate::Original as i32,
    };
    let key = std::slice::from_raw_parts(source, length as usize);
    let entry = match table.get(key) {
        Some(entry) => entry,
        None => return Translate::Original as i32,
    };

    // 名称与参数译文冲突时拒绝替换，而不是猜一个。
    if let (Some(name), Some(param)) = (&entry.name, &entry.param) {
        if name != param {
            return Translate::Ambiguous as i32;
        }
    }

    let value = match (enabled & 1 != 0, enabled & 2 != 0) {
        (true, _) if entry.name.is_some() => entry.name.as_ref(),
        (_, true) if entry.param.is_some() => entry.param.as_ref(),
        _ => None,
    };
    let value = match value {
        Some(value) => value,
        None => return Translate::Original as i32,
    };

    if value.len() + 1 > capacity as usize {
        return Translate::BufferTooSmall as i32;
    }
    std::ptr::copy_nonoverlapping(value.as_ptr(), output, value.len());
    *output.add(value.len()) = 0;
    Translate::Translated as i32
}

/// `translate_raw` 的安全包装，供自检程序使用。
pub fn translate(source: &[u16], output: &mut [u16]) -> i32 {
    unsafe {
        translate_raw(
            source.as_ptr(),
            source.len() as u32,
            output.as_mut_ptr(),
            output.len() as u32,
        )
    }
}
