const root='/ui-fixtures';
const callbacks=new Map(),listeners=new Map(),pending=new Map(),counts=new Map();let callbackId=0;
const size=1024**3;
const prefs={scan_roots:[root],exclusions:[],developer_roots:[],developer_days:7,large_min_mb:50,monitor_enabled:true};
const app={key:'demo',name:'示例编辑器',pkg_type:'manual',version:'1.0',size:'1 GB',size_bytes:size,description:'用于隔离交互验证的应用',icon:'',desktop_file:'',is_system:false,has_ui:true,can_uninstall:true,install_path:root+'/Editor.app',uninstall_note:'预览后确认卸载范围'};
const file={path:root+'/Videos/movie.mp4',name:'movie.mp4',size_bytes:size,size_str:'1 GB',ext:'mp4'};
const duplicate={hash:'a'.repeat(64),size_bytes:size,size_str:'1 GB',files:[root+'/Downloads/photo.png',root+'/Backup/photo.png',root+'/Desktop/photo.png']};
const categories=[{id:'app_cache',title:'应用缓存',desc:'',count:1,total_size_bytes:size,total_size_str:'1 GB',items:[{id:'cache',path:root+'/Caches/editor',name:'编辑器缓存',reason:'可重新生成的缓存',can_delete:true,selected:false,size_known:true,size_bytes:size,size_str:'1 GB',files:[]}]}];
const update={packages:[{name:'example-package',manager:'apt',current_version:'1.0',new_version:'1.1',arch:'arm64',repository:'隔离来源'}],tools:[{id:'tool',name:'示例工具',provider:'npm',current_version:'1.0',latest_version:'1.1',status:'available',install_path:root+'/bin/demo',source_url:'',update_hint:'npm update -g example-tool',note:'隔离预览，不会安装'}],channels:[{id:'apt',provider:'apt',state:'completed',message:'已检测',checked_at:new Date().toISOString(),source_path:root,cache_seconds:0},{id:'npm',provider:'npm',state:'completed',message:'已检测',checked_at:new Date().toISOString(),source_path:root,cache_seconds:0},{id:'brew',provider:'brew',state:'failed',message:'隔离的失败提示',checked_at:new Date().toISOString(),source_path:root,cache_seconds:0}],warnings:['部分渠道未完成，结果可能不完整'],checked_at:new Date().toISOString(),expires_at:new Date(Date.now()+600000).toISOString(),executable_tools:['tool'],automatic_managers:['apt']};
function emit(event,payload){for(const id of listeners.get(event)||[])callbacks.get(id)?.({event,payload});}
function clone(value){return structuredClone(value);}
async function result(cmd,args){
 switch(cmd){
 case 'get_platform_info':return {os:'linux',desktop_visibility:true,autostart_note:'只调整登录启动，不会卸载应用。',updates_note:'',authorization_cache_available:true};
 case 'get_preferences':return clone(prefs);
 case 'save_preferences':Object.assign(prefs,args.preferences);return clone(prefs);
 case 'get_tasks':return [];
 case 'get_monitor_snapshot':return {sampled_at:Date.now(),cpu_percent:12.5,memory_total:16*size,memory_used:8*size,disk_total:512*size,disk_available:128*size,disk_path:root,warnings:[]};
 case 'get_storage_roots':return [root,root+'/Downloads'];
 case 'analyze_disk':return {directories:[{path:args.root+'/Videos',size_bytes:4*size,size:'4 GB',is_dir:true},{path:args.root+'/Downloads',size_bytes:size,size:'1 GB',is_dir:true},{path:args.root+'/notes.pdf',size_bytes:500000,size:'488 KB',is_dir:false}],warning:'',cancelled:false};
 case 'scan_large_files':return {items:[file,{...file,path:root+'/Downloads/archive.zip',name:'archive.zip',ext:'zip'}],roots:[root],warnings:['一个隔离目录无读取权限'],cancelled:false};
 case 'scan_duplicate_files':return {items:[duplicate],roots:[root],warnings:[],cancelled:false};
 case 'scan_junk':return clone(categories);
 case 'scan_developer_junk':return {...clone(categories[0]),id:'developer',title:'开发者垃圾'};
 case 'get_clean_item_details':return [{name:'cache.bin',path:args.path+'/cache.bin',size_bytes:size,size_str:'1 GB'}];
 case 'get_installed_apps_report':return {apps:[app,{...app,key:'protected',name:'系统工具',is_system:true,can_uninstall:false}],channels:[{id:'manual',status:'ok',count:2,note:'隔离目录'}],warnings:['一处隔离来源读取失败'],checked_at:new Date().toISOString()};
 case 'get_app_profile':return {app,caches:[],startup:[],update:null,notes:['仅用于隔离预览']};
 case 'get_manual_uninstall_plan':return {id:'plan',name:app.name,note:'程序移入废纸篓；配置默认保留。',items:[{path:app.install_path,name:'程序',kind:'program',size_bytes:size,size:'1 GB',selected:true,required:true,owners:[],group:'程序'},{path:root+'/Config',name:'个人配置',kind:'config',size_bytes:1024,size:'1 KB',selected:false,required:false,owners:[],group:'配置'}]};
 case 'get_autostart_items':return [{path:root+'/autostart/demo.desktop',name:'示例启动项',exec:root+'/bin/demo --background',comment:'隔离样本',enabled:true,is_user:true}];
 case 'get_desktop_entries':return [{path:root+'/demo.desktop',name:'示例编辑器',exec:root+'/bin/demo',basename:'demo.desktop',scope:'当前用户',visible:true}];
 case 'toggle_autostart_item':case 'toggle_desktop_entry':return;
 case 'check_software_updates':return clone(update);
 case 'plugin:opener|reveal_item_in_dir':return;
 default:throw new Error('隔离预览已拦截：'+cmd);
 }
}
globalThis.isTauri=true;
globalThis.__TAURI_INTERNALS__={metadata:{currentWindow:{label:'main'}},transformCallback(fn){callbacks.set(++callbackId,fn);return callbackId},async invoke(cmd,args={}){
 if(cmd==='plugin:event|listen'){listeners.set(args.event,[...(listeners.get(args.event)||[]),args.handler]);return args.handler;}
 if(cmd==='cancel_task'){const scan=pending.get(args.id);if(scan){emit('task-progress',{...scan.task,state:'cancelling',updated_at:Date.now()});setTimeout(scan.finish,900);}return;}
 if(cmd.startsWith('plugin:window|'))return;
 if(args.taskId){
  const count=(counts.get(cmd)||0)+1;counts.set(cmd,count);
  const task={id:args.taskId,kind:cmd,state:'running',phase:'读取隔离目录',current:246,total:null,path:root+'/example',started_at:Date.now(),updated_at:Date.now(),cancellable:true};emit('task-progress',task);
  return new Promise((resolve,reject)=>{let timer;const finish=()=>{clearTimeout(timer);pending.delete(args.taskId);Promise.resolve(result(cmd,args)).then(resolve,reject)};pending.set(args.taskId,{finish,task});timer=setTimeout(finish,count===1?350:120000)});
 }
 return result(cmd,args);
}};
globalThis.__TAURI_EVENT_PLUGIN_INTERNALS__={unregisterListener(){}};
for(const event of ['error','unhandledrejection'])window.addEventListener(event,e=>{document.querySelector('#review-errors').textContent=String(e.message||e.reason);});
await import('/src/main.ts');
