import {forwardRef,useEffect,useImperativeHandle,useRef,type MouseEventHandler} from 'react';
import type {Kind,Metadata} from './types';
export interface ViewerHandle {fit:()=>void;actual:()=>void}
interface Props {bitmap:ImageBitmap|null;photoId:string;meta:Metadata|null;kind:Kind|null;error?:string;onZoom:(z:number)=>void;onContextMenu:MouseEventHandler<HTMLDivElement>}
// Transient pointer coordinates live outside React. Only this canvas redraws per frame.
export default forwardRef<ViewerHandle,Props>(function Viewer({bitmap,photoId,meta,kind,error,onZoom,onContextMenu},ref){
 const host=useRef<HTMLDivElement>(null),canvas=useRef<HTMLCanvasElement>(null);
 const state=useRef({bitmap:null as ImageBitmap|null,width:1,height:1,w:1,h:1,cx:.5,cy:.5,zoom:1,fit:true,raf:0,drag:false,x:0,y:0});
 const notify=useRef(onZoom);notify.current=onZoom;
 const draw=()=>{
   const started=performance.now();
   const s=state.current;s.raf=0;const c=canvas.current;if(!c)return;const ctx=c.getContext('2d',{alpha:false});if(!ctx)return;
   const dpr=window.devicePixelRatio||1;const fit=Math.min(s.w/s.width,s.h/s.height)*dpr;
   const z=s.fit?fit:s.zoom;const scale=z/dpr;
   ctx.setTransform(dpr,0,0,dpr,0,0);ctx.fillStyle='#242424';ctx.fillRect(0,0,s.w,s.h);
   if(s.bitmap&&s.bitmap.width){ctx.imageSmoothingEnabled=true;ctx.imageSmoothingQuality='high';ctx.drawImage(s.bitmap,s.w/2-s.cx*s.width*scale,s.h/2-s.cy*s.height*scale,s.width*scale,s.height*scale)}
   notify.current(Math.round(z*100));
   if(new URLSearchParams(location.search).has('qa')){c.dataset.drawMs=(performance.now()-started).toFixed(2);c.dataset.scale=String(z);c.dataset.source=s.bitmap?`${s.bitmap.width}x${s.bitmap.height}`:'';c.dataset.center=`${s.cx.toFixed(4)},${s.cy.toFixed(4)}`;}
 };
 const schedule=()=>{if(!state.current.raf)state.current.raf=requestAnimationFrame(draw)};
 const clamp=()=>{const s=state.current,dpr=devicePixelRatio||1,scale=s.zoom/dpr;const rx=Math.min(.5,s.w/(2*s.width*scale)),ry=Math.min(.5,s.h/(2*s.height*scale));s.cx=Math.max(rx,Math.min(1-rx,s.cx));s.cy=Math.max(ry,Math.min(1-ry,s.cy))};
 const fit=()=>{Object.assign(state.current,{fit:true,cx:.5,cy:.5});schedule()};
 const actual=()=>{state.current.fit=false;state.current.zoom=1;clamp();schedule()};
 useImperativeHandle(ref,()=>({fit,actual}));
 useEffect(()=>{
  const s=state.current;s.bitmap=bitmap;
  if(bitmap&&(kind==='full'||kind==='raw')){s.width=bitmap.width;s.height=bitmap.height}else if(meta){s.width=meta.width;s.height=meta.height}else if(bitmap){s.width=bitmap.width;s.height=bitmap.height}
  schedule();
 },[bitmap,meta,photoId,kind]);
 useEffect(()=>{
  const el=host.current!,c=canvas.current!;
  const size=()=>{const r=el.getBoundingClientRect(),s=state.current,dpr=devicePixelRatio||1;s.w=r.width;s.h=r.height;c.width=Math.round(r.width*dpr);c.height=Math.round(r.height*dpr);clamp();schedule()};
  const observer=new ResizeObserver(size);observer.observe(el);size();
  const wheel=(e:WheelEvent)=>{e.preventDefault();const s=state.current;if(!s.bitmap)return;const r=el.getBoundingClientRect(),dpr=devicePixelRatio||1,old=s.fit?Math.min(s.w/s.width,s.h/s.height)*dpr:s.zoom;const next=Math.max(.03,Math.min(8,old*Math.exp(-e.deltaY*.0015)));const px=e.clientX-r.left-s.w/2,py=e.clientY-r.top-s.h/2;s.cx+=px*dpr/s.width*(1/old-1/next);s.cy+=py*dpr/s.height*(1/old-1/next);s.zoom=next;s.fit=false;clamp();schedule()};
  const down=(e:PointerEvent)=>{if(e.button!==0)return;const s=state.current;s.drag=true;s.x=e.clientX;s.y=e.clientY;el.setPointerCapture(e.pointerId);el.classList.add('dragging')};
  const move=(e:PointerEvent)=>{const s=state.current;if(!s.drag||s.fit)return;const dpr=devicePixelRatio||1;s.cx-=(e.clientX-s.x)*dpr/(s.width*s.zoom);s.cy-=(e.clientY-s.y)*dpr/(s.height*s.zoom);s.x=e.clientX;s.y=e.clientY;clamp();schedule()};
  const up=()=>{state.current.drag=false;el.classList.remove('dragging')};
  const dbl=()=>{state.current.fit?actual():fit()};
  el.addEventListener('wheel',wheel,{passive:false});el.addEventListener('pointerdown',down);el.addEventListener('pointermove',move);el.addEventListener('pointerup',up);el.addEventListener('pointercancel',up);el.addEventListener('dblclick',dbl);
  return()=>{observer.disconnect();cancelAnimationFrame(state.current.raf);state.current.raf=0;el.removeEventListener('wheel',wheel);el.removeEventListener('pointerdown',down);el.removeEventListener('pointermove',move);el.removeEventListener('pointerup',up);el.removeEventListener('pointercancel',up);el.removeEventListener('dblclick',dbl)};
 },[]);
 return <div className="viewer" ref={host} tabIndex={0} onContextMenu={onContextMenu} aria-label="照片画布：滚轮缩放，拖动平移，双击切换 100%"><canvas ref={canvas}/>{!bitmap&&<span className="canvas-message">{error||'正在读取照片…'}</span>}</div>;
});
