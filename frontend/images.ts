import {imageUrl} from './api';
import type {Kind} from './types';
type Entry={bitmap:ImageBitmap;bytes:number};
const cache=new Map<string,Entry>();
const pending=new Map<string,Promise<ImageBitmap>>();
let total=0;
const budget=384*1024*1024;
let pinned=new Set<string>();
let generation=0;
export const imageKey=(id:string,kind:Kind)=>id+':'+kind;
export function peek(id:string,kind:Kind){const key=imageKey(id,kind),e=cache.get(key);if(e){cache.delete(key);cache.set(key,e)}return e?.bitmap}
export function pin(id:string){pinned=new Set([imageKey(id,'preview'),imageKey(id,'full'),imageKey(id,'raw')]);prune()}
function prune(){for(const [key,e]of cache){if(total<=budget)break;if(pinned.has(key))continue;cache.delete(key);total-=e.bytes;e.bitmap.close()}}
export function resetImages(){generation++;for(const e of cache.values())e.bitmap.close();cache.clear();pending.clear();pinned.clear();total=0}
export async function loadImage(id:string,kind:Kind):Promise<ImageBitmap>{
 const key=imageKey(id,kind),hit=peek(id,kind);if(hit)return hit;
 const existing=pending.get(key);if(existing)return existing;
 const epoch=generation;
 const promise=(async()=>{
   const response=await fetch(imageUrl(id,kind));if(!response.ok)throw Error(await response.text());
   let bitmap:ImageBitmap;
   if(kind==='raw'){
     const buffer=await response.arrayBuffer();const view=new DataView(buffer);
     if(buffer.byteLength<12||view.getUint32(0,true)!==0x3156524e)throw Error('RAW 图像响应无效');
     const w=view.getUint32(4,true),h=view.getUint32(8,true);
     if(buffer.byteLength!==12+w*h*4)throw Error('RAW 图像长度无效');
     bitmap=await createImageBitmap(new ImageData(new Uint8ClampedArray(buffer,12),w,h));
   }else {const blob=await response.blob();try{bitmap=await createImageBitmap(blob)}catch{throw Error('无法解码此图片，文件可能损坏或格式暂不受支持。')}}
   if(epoch!==generation){bitmap.close();throw Error('照片来源已改变')}
   const bytes=bitmap.width*bitmap.height*4;cache.set(key,{bitmap,bytes});total+=bytes;prune();return bitmap;
 })();pending.set(key,promise);
 try{return await promise}finally{if(pending.get(key)===promise)pending.delete(key)}
}
export function cacheStats(){return {bytes:total,entries:cache.size,pending:pending.size}}
