import {useEffect,useRef,useState,type RefObject} from 'react';
import type {Photo} from './types';

type Drag={pointerId:number;from:number;to:number;startX:number;startY:number;x:number;y:number;started:boolean};
const DRAG_DISTANCE=6;

// Track pointer coordinates outside React. Only a changed photo endpoint updates
// the local preview; the collection is changed once, when the mouse is released.
export function useRangeDrag(host:RefObject<HTMLDivElement|null>,photos:Photo[],enabled:boolean,onRange?:((from:string,to:string)=>void)){
 const [preview,setPreview]=useState<readonly [number,number]|null>(null);
 const commit=useRef(onRange);commit.current=onRange;
 useEffect(()=>{
  const el=host.current;if(!enabled||!el)return;
  let drag:Drag|null=null,frame=0,lastFrame=0,blockClick=false;
  const hit=(x:number,y:number)=>{
   const cell=document.elementFromPoint(x,y)?.closest<HTMLElement>('[data-photo-index]');
   if(!cell||!el.contains(cell))return -1;
   const index=Number(cell.dataset.photoIndex);
   return Number.isInteger(index)&&index>=0&&index<photos.length?index:-1;
  };
  const finish=()=>{
   const old=drag;drag=null;cancelAnimationFrame(frame);frame=0;lastFrame=0;
   el.classList.remove('range-selecting');setPreview(null);
   if(old&&el.hasPointerCapture(old.pointerId))el.releasePointerCapture(old.pointerId);
  };
  const updatePreview=(to:number)=>{
   if(!drag||to<0||to===drag.to)return;
   drag.to=to;setPreview([Math.min(drag.from,to),Math.max(drag.from,to)]);
  };
  const tick=(time:number)=>{
   const current=drag;if(!current?.started)return;
   const r=el.getBoundingClientRect(),dt=Math.min(32,lastFrame?time-lastFrame:16)/1000;lastFrame=time;
   // Continue scrolling even when the pointer is stationary near an edge.
   if(current.x>=r.left&&current.x<r.right-16&&current.y>=r.top-24&&current.y<=r.bottom+24){
    const edge=48;
    const speed=current.y<r.top+edge?-800*Math.min(1,(r.top+edge-current.y)/edge):current.y>r.bottom-edge?800*Math.min(1,(current.y-r.bottom+edge)/edge):0;
    if(speed)el.scrollTop+=speed*dt;
    updatePreview(hit(current.x,Math.max(r.top+10,Math.min(r.bottom-10,current.y))));
   }
   frame=requestAnimationFrame(tick);
  };
  const down=(e:PointerEvent)=>{
   blockClick=false;
   if(drag)finish();
   if(e.button!==0||e.pointerType!=='mouse'||!e.isPrimary)return;
   const target=e.target as Element;
   if(!el.contains(target))return;
   const from=hit(e.clientX,e.clientY);if(from<0)return;
   drag={pointerId:e.pointerId,from,to:from,startX:e.clientX,startY:e.clientY,x:e.clientX,y:e.clientY,started:false};
  };
  const move=(e:PointerEvent)=>{
   if(!drag||e.pointerId!==drag.pointerId)return;
   if(!(e.buttons&1)){finish();return;}
   drag.x=e.clientX;drag.y=e.clientY;
   if(!drag.started&&Math.hypot(drag.x-drag.startX,drag.y-drag.startY)>=DRAG_DISTANCE){
    drag.started=true;blockClick=true;el.classList.add('range-selecting');setPreview([drag.from,drag.from]);
    // Capture on the scroll host, which stays mounted when virtual rows leave.
    el.setPointerCapture(drag.pointerId);frame=requestAnimationFrame(tick);
   }
   if(drag.started)e.preventDefault();
  };
  const up=(e:PointerEvent)=>{
   if(!drag||e.pointerId!==drag.pointerId)return;
   const current=drag,to=hit(e.clientX,e.clientY);
   if(current.started){
    blockClick=true;
    // Releasing outside a photo cancels rather than selecting an unseen endpoint.
    if(to>=0)commit.current?.(photos[current.from].id,photos[to].id);
   }
   finish();
  };
  const cancel=(e:PointerEvent)=>{if(drag&&e.pointerId===drag.pointerId){blockClick=drag.started;finish()}};
  const blur=()=>{if(drag){blockClick=drag.started;finish()}};
  const key=(e:KeyboardEvent)=>{
   if(drag&&e.key==='Escape'){blockClick=true;e.preventDefault();e.stopImmediatePropagation();finish()}
  };
  const click=(e:MouseEvent)=>{
   // A drag must not be followed by the label/checkbox's normal toggle click.
   if(blockClick&&e.detail>0){blockClick=false;e.preventDefault();e.stopImmediatePropagation()}
  };
  window.addEventListener('pointerdown',down,true);
  window.addEventListener('pointermove',move,{capture:true,passive:false});
  window.addEventListener('pointerup',up,true);
  window.addEventListener('pointercancel',cancel,true);
  window.addEventListener('blur',blur);
  window.addEventListener('keydown',key,true);
  window.addEventListener('click',click,true);
  el.addEventListener('lostpointercapture',cancel);
  return()=>{
   finish();
   window.removeEventListener('pointerdown',down,true);window.removeEventListener('pointermove',move,true);
   window.removeEventListener('pointerup',up,true);window.removeEventListener('pointercancel',cancel,true);
   window.removeEventListener('blur',blur);window.removeEventListener('keydown',key,true);window.removeEventListener('click',click,true);
   el.removeEventListener('lostpointercapture',cancel);
  };
 },[host,photos,enabled]);
 return preview;
}
