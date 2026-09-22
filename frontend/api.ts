import {Channel,invoke} from '@tauri-apps/api/core';
import {open,save} from '@tauri-apps/plugin-dialog';
import type {Collection,DeleteReport,Metadata,ExportAction,ExportProgress,ExportReport,Photo} from './types';
export const desktop = '__TAURI_INTERNALS__' in window;
export const qa = !desktop && new URLSearchParams(location.search).has('qa');
export const demo = qa && new URLSearchParams(location.search).has('demo');
const fixture=()=>import('./qa-fixture');
const base=desktop?'http://nbphoto.localhost':'http://127.0.0.1:1421';
export const imageUrl=(id:string,kind:string)=>`${base}/image/${id}/${kind}`;
async function json<T>(path:string):Promise<T>{const r=await fetch(base+path);if(!r.ok)throw Error(await r.text());return r.json()}
export const metadata=(id:string)=>json<Metadata>(`/metadata/${id}`);
export async function choose(folder:boolean):Promise<Collection|null>{
  if(!desktop){if(qa){const source=await json<Collection>('/collection');return demo?(await fixture()).choose(source):source}throw Error('请在 NBPhotoViewer 桌面窗口中选择本地文件。')}
  const formats=await invoke<{images:string[];raw:string[]}>('supported_formats');
  const paths=await open({directory:folder,multiple:!folder,title:folder?'选择照片文件夹':'选择照片',filters:folder?undefined:[{name:'所有支持的照片',extensions:[...formats.images,...formats.raw]},{name:'常见图片',extensions:formats.images},{name:'相机 RAW',extensions:formats.raw}]});
  if(!paths)return null;
  return invoke<Collection>('select_sources',{paths:Array.isArray(paths)?paths:[paths]});
}
export async function markPhoto(id:string,marked:boolean){if(demo){(await fixture()).mark(id,marked);return}if(!desktop)throw Error('浏览器验证模式为只读，请在桌面程序中标记。');await invoke('set_mark',{id,marked})}
export async function trashPhoto(id:string){if(!desktop)throw Error('浏览器验证模式为只读，请在桌面程序中删除。');await invoke('trash_photo',{id})}
export async function trashPhotos(ids:string[]):Promise<DeleteReport>{
  if(demo)return (await fixture()).remove(ids);
  if(!desktop)throw Error('浏览器验证模式为只读，请在桌面程序中删除。');
  return invoke<DeleteReport>('trash_photos',{ids});
}
export async function exportDestination(action:ExportAction,name:string):Promise<string|null>{
  if(demo)return action==='copy'?'':action==='save'?`演示导出/${name.replace(/\.[^.]+$/, '')}.jpg`:'演示导出';
  if(!desktop)throw Error('请在桌面程序中导出。');
  if(action==='copy')return '';
  if(action==='save')return save({title:'将图片另存为 JPEG',defaultPath:name.replace(/\.[^.]+$/, '')+'.jpg',filters:[{name:'JPEG 图片',extensions:['jpg']}]});
  if(action==='zip')return save({title:'导出并压缩 · 输入名称，自动追加编号',defaultPath:await invoke<string>('zip_export_default'),filters:[{name:'ZIP 压缩包',extensions:['zip']}]});
  const result=await open({title:'选择 JPEG 导出文件夹',directory:true,multiple:false});
  return typeof result==='string'?result:null;
}
export async function exportPhotos(action:ExportAction,photos:Photo[],preferRaw:boolean,path:string,jobId:string,progress:(p:ExportProgress)=>void):Promise<ExportReport>{
  if(demo)return (await fixture()).exportDemo(action,photos,jobId,progress);
  if(!desktop)throw Error('请在桌面程序中导出。');
  if(action==='zip'||action==='jpeg'){
    const channel=new Channel<ExportProgress>();channel.onmessage=progress;
    return invoke<ExportReport>(action==='zip'?'export_zip':'export_jpegs',{ids:photos.map(p=>p.id),...(action==='zip'?{path}:{directory:path}),jobId,onProgress:channel});
  }
  return invoke<ExportReport>(action==='copy'?'copy_photo':'save_photo_jpeg',{id:photos[0].id,preferRaw,path,jobId});
}
export async function cancelExport(jobId:string){if(demo){(await fixture()).cancelExportDemo(jobId);return}await invoke('cancel_export',{jobId})}
