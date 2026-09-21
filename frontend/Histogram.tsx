import {memo,useEffect,useLayoutEffect,useRef,useState} from 'react';
import type {Kind} from './types';
import type {HistogramData} from './histogramMath';
import {loadHistogram,peekHistogram} from './histograms';
type Mode='luminance'|'rgb';
type Result={key:string;data?:HistogramData;error?:string;ms?:number;cached?:boolean};
function draw(canvas:HTMLCanvasElement,data:HistogramData|undefined,mode:Mode){
 const box=canvas.getBoundingClientRect(),ratio=window.devicePixelRatio||1;
 canvas.width=Math.max(1,Math.round(box.width*ratio));canvas.height=Math.max(1,Math.round(box.height*ratio));
 const ctx=canvas.getContext('2d');if(!ctx)return;
 ctx.scale(ratio,ratio);const w=box.width,h=box.height;
 ctx.fillStyle='#292929';ctx.fillRect(0,0,w,h);ctx.strokeStyle='#454545';ctx.lineWidth=1;ctx.beginPath();
 for(let i=1;i<8;i++){const x=Math.round(w*i/8)+.5;ctx.moveTo(x,0);ctx.lineTo(x,h)}
 for(let i=1;i<4;i++){const y=Math.round(h*i/4)+.5;ctx.moveTo(0,y);ctx.lineTo(w,y)}
 ctx.stroke();if(!data?.samples)return;
 const channels=mode==='luminance'?[data.luminance]:[data.red,data.green,data.blue];
 const fills=mode==='luminance'?['#cfcfcf']:['#d96b6680','#72b37b80','#6b99db80'];
 const strokes=mode==='luminance'?['#dedede']:['#d9847f','#8aba91','#87a9dc'];
 let max=1;for(const channel of channels)for(const count of channel)max=Math.max(max,count);
 channels.forEach((channel,j)=>{
  ctx.beginPath();ctx.moveTo(0,h);
  for(let i=0;i<256;i++){const x=i*w/256,y=h-1-channel[i]/max*(h-6);ctx.lineTo(x,y);ctx.lineTo((i+1)*w/256,y)}
  ctx.lineTo(w,h);ctx.closePath();ctx.fillStyle=fills[j];ctx.fill();ctx.strokeStyle=strokes[j];ctx.lineWidth=.75;ctx.stroke();
 });
}
export default memo(function Histogram({photoId,bitmap,kind,isRaw,imageError}:{photoId:string;bitmap:ImageBitmap|null;kind:Kind|null;isRaw:boolean;imageError?:string}){
 const [mode,setMode]=useState<Mode>('luminance'),[result,setResult]=useState<Result|null>(null),[retry,setRetry]=useState(0);
 const canvas=useRef<HTMLCanvasElement>(null),key=photoId+':'+(kind==='raw'?'raw':'camera');
 const current=result?.key===key&&bitmap?result:null;
 useEffect(()=>{
  if(!bitmap||!kind)return;
  const hit=peekHistogram(key);if(hit){setResult({key,data:hit,ms:0,cached:true});return}
  setResult(null);const controller=new AbortController();
  // Let the photo paint first; quick navigation cancels work before it begins.
  const timer=window.setTimeout(()=>{
   const start=performance.now();
   void loadHistogram(key,bitmap,controller.signal).then(data=>{
    if(!controller.signal.aborted)setResult({key,data,ms:Math.round(performance.now()-start),cached:false});
   }).catch(error=>{if(!controller.signal.aborted)setResult({key,error:error instanceof Error?error.message:String(error)})});
  },90);
  return()=>{clearTimeout(timer);controller.abort()};
 },[key,bitmap,kind,retry]);
 useLayoutEffect(()=>{
  const element=canvas.current;if(!element)return;
  const render=()=>draw(element,current?.data,mode);render();
  const observer=new ResizeObserver(render);observer.observe(element);return()=>observer.disconnect();
 },[current?.data,mode]);
 const source=kind==='raw'?'RAW 显影':isRaw?'RAW 预览':'原图';
 return <section className="histogram" aria-label="照片直方图" data-photo-id={photoId} data-histogram-state={current?.data?'ready':current?.error||imageError?'error':'loading'} data-histogram-ms={current?.ms} data-histogram-cached={current?.cached} data-histogram-samples={current?.data?.samples}>
  <div className="histogram-heading"><h2>直方图</h2><div className="histogram-modes" role="group" aria-label="直方图通道">
   <button aria-pressed={mode==='luminance'} className={mode==='luminance'?'active':''} onClick={()=>setMode('luminance')}>亮度</button>
   <button aria-pressed={mode==='rgb'} className={mode==='rgb'?'active':''} onClick={()=>setMode('rgb')}>RGB</button>
  </div></div>
  <div className="histogram-plot" title="横轴从暗到亮（0–255），纵轴为像素数量。RGB 共用频次尺度；若某个通道有很高的峰值，其他曲线会相对较低。"><canvas ref={canvas} role="img" aria-label={`${mode==='luminance'?'亮度':'红绿蓝'}直方图，左侧暗部，右侧亮部，范围 0 至 255${current?.data?'':'，尚未就绪'}`}/>
   {!current?.data&&<div className="histogram-message" role="status">{imageError?<span>暂不可用</span>:current?.error?<><span title={current.error}>暂不可用</span><button onClick={()=>{setResult(null);setRetry(n=>n+1)}}>重试</button></>:'正在计算…'}</div>}
  </div>
  <p className="histogram-source" title="基于当前预览整图缩小采样，最长边 1024 像素；每通道 256 档。缩放和拖动不改变统计范围，不代表未经显影的 RAW 传感器数据。">{source} · 全图</p>
 </section>;
});
