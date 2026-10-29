#include "backend/json.jsxinc"
#include "backend/dictionary-manager.jsxinc"
(function(host){
 function now(){return (new Date()).getTime();}
 var root=(typeof KOSUZU_PACKAGE_ROOT==='string')?new Folder(KOSUZU_PACKAGE_ROOT):new File($.fileName).parent,sharedRuntime=new Folder(root.fsName+'/runtime'),runtime=sharedRuntime;
 var docked=host instanceof Panel,win=docked?host:new Window('palette','本居小铃 Kosuzu 0.8.3 · 离线原生版',undefined,{resizeable:true});
 win.orientation='column';win.alignChildren=['fill','top'];win.margins=4;win.spacing=2;
 var row=win.add('group');row.spacing=6;var params=row.add('checkbox',undefined,'参数'),names=row.add('checkbox',undefined,'名称'),dictionaryButton=row.add('button',undefined,'词典');dictionaryButton.preferredSize=[45,24];
 var info=win.add('statictext',undefined,'正在连接原生引擎…',{multiline:true});info.preferredSize=[280,36];info.visible=true;
 var current={parameters:false,names:false},pending=false,connectedPid=0,starting=true,startTime=now(),alive=true;
 function report(s){info.text=s;row.helpTip=s;params.helpTip=s;names.helpTip=s;}
 function read(file){file.encoding='UTF-8';if(!file.open('r'))throw Error('无法读取 '+file.fsName);try{return LBJSON.parse(file.read());}finally{file.close();}}
 function refresh(){try{var config=read(new File(root.fsName+'/environment.json'));if(config.nativeBackendEnabled!==true)throw Error('原生引擎未启用，词典编辑仍可使用');if(!connectedPid)throw Error('正在连接原生引擎…');var s=read(new File(runtime.fsName+'/status.json'));if(!s.running||s.pid!==connectedPid||now()-s.heartbeat>5000)throw Error(s.error||'引擎未连接');current=s.state;params.value=current.parameters;names.value=current.names;params.enabled=names.enabled=!pending&&!s.error;report(s.error||('已连接 AE '+s.pid+' · '+s.rules+' 条词典'));return s;}catch(e){params.value=names.value=false;params.enabled=names.enabled=false;report(e.toString());return null;}}
 function connect(){try{var config=read(new File(root.fsName+'/environment.json'));if(config.nativeBackendEnabled!==true){starting=false;refresh();return;}if(!sharedRuntime.exists&&!sharedRuntime.create())throw Error('请在 AE 首选项 → 脚本和表达式中允许脚本写入文件');var launcher=new File(root.fsName+'/backend/KosuzuController.exe');if(!launcher.exists||!launcher.execute())throw Error('原生组件启动失败，请重新安装完整包');}catch(e){starting=false;report(e.toString());}}
 function poll(){if(!alive)return;if(starting){try{var c=read(new File(sharedRuntime.fsName+'/connection.json'));if(c.created>=startTime-1000){if(!c.ok){starting=false;throw Error(c.error);}connectedPid=c.pid;runtime=new Folder(sharedRuntime.fsName+'/ae-'+connectedPid);if(refresh())starting=false;}}catch(e){if(!starting)report(e.toString());}if(starting&&now()-startTime>20000){starting=false;report('引擎未连接。请确认只打开一个AE，并检查安装包是否完整。');}}else if(connectedPid)refresh();$.global.LBPanelTaskId=app.scheduleTask('$.global.LBPanelTick()',starting?250:2000,false);}
 function command(method,state,success,failure){
  failure=failure||function(e){report(e.toString());};
  if(pending){failure(Error('上一操作仍在等待确认'));return;}if(!refresh()){failure(Error('显示引擎未连接；本地词典仍可保存'));return;}
  var id=now().toString(36)+'-'+Math.floor(Math.random()*1000000),file=new File(runtime.fsName+'/requests/'+id+'.tmp');
  try{file.encoding='UTF-8';if(!file.open('w'))throw Error('AE未允许脚本写文件');try{file.write(LBJSON.stringify({id:id,created:now(),method:method,state:state||null}));}finally{file.close();}if(!file.rename(id+'.json'))throw Error('提交失败');}catch(e){failure(e);return;}
  pending=true;params.enabled=names.enabled=false;report('正在等待引擎确认…');
  if(!$.global.LBPendingTasks)$.global.LBPendingTasks={};var start=now(),answer=new File(runtime.fsName+'/responses/'+id+'.json');
  function finish(error,result){delete $.global.LBPendingTasks[id];pending=false;if(error){refresh();failure(error);}else{if(result&&result.state){current=result.state;params.value=current.parameters;names.value=current.names;}refresh();if(success)success(result);}}
  $.global.LBPendingTasks[id]=function(){try{if(answer.exists){var response=read(answer);answer.remove();if(response.id!==id||!response.ok)throw Error(response.error||'确认不匹配');finish(null,response.result);return;}if(now()-start>8000)throw Error('引擎响应超时，界面保持可操作');app.scheduleTask('$.global.LBPendingTasks["'+id+'"]()',100,false);}catch(e){finish(e);}};
  app.scheduleTask('$.global.LBPendingTasks["'+id+'"]()',100,false);
 }
 function guard(action){return function(){try{action();}catch(e){var message=e.toString()+' · 行 '+e.line;report(message);try{var log=new File(sharedRuntime.fsName+'/ui-error.log');log.encoding='UTF-8';if(log.open('a')){log.writeln((new Date()).toString()+' '+message);log.close();}}catch(ignore){}alert('Kosuzu：'+message);}};}
 params.onClick=guard(function(){var next=params.value;command('set',{parameters:next,names:current.names},function(){report('后端已确认。');});});
 names.onClick=guard(function(){var next=names.value;command('set',{parameters:current.parameters,names:next},function(){report('后端已确认。');});});
 dictionaryButton.onClick=guard(function(){LBOpenDictionaryManager(root,function(ok,fail){command('reload',null,ok,fail);},function(ok,fail){command('stop',null,ok,fail);});});
 params.enabled=names.enabled=false;win.onResizing=win.onResize=function(){this.layout.resize();};win.onClose=function(){alive=false;};win.layout.layout(true);if(win instanceof Window){win.center();win.show();}
 try{if($.global.LBPanelTaskId)app.cancelTask($.global.LBPanelTaskId);}catch(ignored){}$.global.LBPanelTick=poll;$.global.LBPanelConnect=function(){connect();poll();};$.global.LBPanelTaskId=app.scheduleTask('$.global.LBPanelConnect()',200,false);
})(this);
