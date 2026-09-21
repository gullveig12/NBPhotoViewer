// Explicit browser-only interaction fixture. No mutations or POST requests reach
// the read-only photo server; the desktop always uses real Tauri commands.
import type {Collection,DeleteReport,ExportAction,ExportProgress,ExportReport,Photo} from './types';
let current:Collection={photos:[],label:''};
let saved=new Map<string,boolean>();
let seeded=false;
let failOnce=true;
export function choose(source:Collection):Collection{
 if(!seeded){
  const indices=[1,4,6,9,14,17,...Array.from({length:30},(_,i)=>36+i*10)];
  indices.forEach(i=>{if(source.photos[i])saved.set(source.photos[i].id,true)});
  seeded=true;
 }
 current={...source,photos:source.photos.map(p=>({...p,marked:saved.get(p.id)??false}))};
 return structuredClone(current);
}
export function mark(id:string,marked:boolean){saved.set(id,marked);const photo=current.photos.find(p=>p.id===id);if(photo)photo.marked=marked}
export async function remove(ids:string[]):Promise<DeleteReport>{
 await new Promise(resolve=>setTimeout(resolve,350));
 const report:DeleteReport={deleted:[],failed:[]};
 const simulateFailure=new URLSearchParams(location.search).has('partial')&&failOnce;
 for(const id of new Set(ids)){
  const p=current.photos.find(p=>p.id===id);
  if(!p){report.failed.push({id,name:id,error:'照片不在当前选择范围内'});continue}
  if(simulateFailure&&report.failed.length===0){report.failed.push({id,name:p.name,error:'演示：文件被占用，请稍后重试。'});continue}
  report.deleted.push(id);
 }
 failOnce=false;
 const deleted=new Set(report.deleted);current.photos=current.photos.filter(p=>!deleted.has(p.id));
 return report;
}
const cancelledJobs=new Set<string>();
export function cancelExportDemo(id:string){cancelledJobs.add(id)}
export async function exportDemo(action:ExportAction,photos:Photo[],id:string,progress:(p:ExportProgress)=>void):Promise<ExportReport>{
 const report:ExportReport={exported:0,total:photos.length,files:[],failed:[],warnings:[],cancelled:false,fatal:null};
 for(const photo of photos){
  progress({completed:report.exported,total:photos.length,name:photo.name,phase:'演示：正在转换 JPEG'});
  await new Promise(resolve=>setTimeout(resolve,450));
  if(cancelledJobs.has(id)){report.cancelled=true;break}
  report.exported++;
 }
 cancelledJobs.delete(id);
 if(report.exported&&action!=='copy')report.files=[action==='zip'?'演示：照片导出-001.zip':'演示：照片.jpg'];
 return report;
}
