import type {HistogramData} from './histogramMath';
// 256 entries use about 1 MiB; no image bitmaps are retained here.
const cache=new Map<string,HistogramData>();
let generation=0;
export function resetHistograms(){generation++;cache.clear()}
export function peekHistogram(key:string){
 const value=cache.get(key);if(value){cache.delete(key);cache.set(key,value)}return value;
}
export async function loadHistogram(key:string,bitmap:ImageBitmap,signal:AbortSignal):Promise<HistogramData>{
 signal.throwIfAborted();
 const hit=peekHistogram(key);if(hit)return hit;
 const epoch=generation;
 // Transfer a clone: transferring the viewer's bitmap would detach its image.
 const copy=await createImageBitmap(bitmap);
 if(signal.aborted){copy.close();signal.throwIfAborted()}
 return new Promise((resolve,reject)=>{
  let worker:Worker|undefined,finished=false;
  const finish=(error?:unknown,data?:HistogramData)=>{
   if(finished)return;finished=true;
   clearTimeout(timeout);signal.removeEventListener('abort',abort);worker?.terminate();copy.close();
   if(error){reject(error);return}
   if(!data){reject(Error('直方图数据为空'));return}
   if(epoch===generation){cache.set(key,data);if(cache.size>256)cache.delete(cache.keys().next().value!)}
   resolve(data);
  };
  const abort=()=>finish(new DOMException('照片已切换','AbortError'));
  const timeout=window.setTimeout(()=>finish(Error('直方图计算超时')),8000);
  signal.addEventListener('abort',abort,{once:true});
  try{
   worker=new Worker(new URL('./histogram.worker.ts',import.meta.url),{type:'module'});
   worker.onmessage=(event:MessageEvent<{data?:HistogramData;error?:string}>)=>finish(event.data.error?Error(event.data.error):undefined,event.data.data);
   worker.onerror=()=>finish(Error('无法计算直方图'));
   worker.onmessageerror=()=>finish(Error('无法读取直方图结果'));
   worker.postMessage({bitmap:copy},[copy]);
  }catch(error){finish(error)}
 });
}
