import {memo,useEffect,useLayoutEffect,useRef,useState} from 'react';
import {imageUrl} from './api';
import type {Photo} from './types';
import type {SelectionGesture} from './batchSelection';
import {useRangeDrag} from './useRangeDrag';
interface Props{photos:Photo[];selected:string;filmstrip?:boolean;onSelect:(i:number)=>void;onOpen:(i:number)=>void;scrollMemory:{current:number};batch?:boolean;checked?:Set<string>;onBatchSelect?:(id:string,gesture:SelectionGesture)=>void;onBatchRange?:(from:string,to:string)=>void;disabled?:boolean}
const Thumb=memo(function Thumb({photo,selected,onClick,onDoubleClick,batch,onBatchSelect,disabled}:{photo:Photo;selected:boolean;onClick:()=>void;onDoubleClick:()=>void;batch:boolean;onBatchSelect:(gesture:SelectionGesture)=>void;disabled:boolean}){
 const [failed,setFailed]=useState(false);
 const checkbox=useRef<HTMLInputElement>(null);
 const activate=(e:{shiftKey:boolean;ctrlKey:boolean;metaKey:boolean})=>{if(!disabled)onBatchSelect({shift:e.shiftKey,additive:e.ctrlKey||e.metaKey})};
 const content=<><div className="thumb-image">{failed?<span>预览不可用</span>:<img src={imageUrl(photo.id,'thumb')} alt="" draggable={false} onError={()=>setFailed(true)}/>} {photo.marked&&<span className="flag" aria-label="已标记"><svg viewBox="0 0 20 20"><path d="M4 18V3h11l-2 4 2 4H5"/></svg></span>}
 {/* Click handles the controlled checkbox so mouse/keyboard modifiers survive. */}
 {batch&&<input ref={checkbox} className="batch-checkbox" type="checkbox" checked={selected} onChange={()=>{}} onClick={e=>{e.stopPropagation();activate(e)}} disabled={disabled} aria-label={`选择 ${photo.name}${photo.marked?'，已标记':''}`} onKeyDown={e=>{if(e.key==='Enter'||e.key===' '){e.preventDefault();activate(e)}}}/>}</div><span className="filename">{photo.name}</span></>;
 const className='thumb'+(selected?' selected':'');
 return batch?<label className={className+' batch-thumb'} title={photo.name} onClick={e=>{e.preventDefault();activate(e);checkbox.current?.focus({preventScroll:true})}}>{content}</label>:<button className={className} onClick={onClick} onDoubleClick={onDoubleClick} onKeyDown={e=>{if(e.key==='Enter'){e.preventDefault();onDoubleClick()}}} disabled={disabled} title={photo.name} aria-label={photo.name+(photo.marked?'，已标记':'')} aria-pressed={selected}>{content}</button>;
});
export default function Thumbnails({photos,selected,filmstrip=false,onSelect,onOpen,scrollMemory,batch=false,checked,onBatchSelect,onBatchRange,disabled=false}:Props){
 const host=useRef<HTMLDivElement>(null);const [box,setBox]=useState({w:800,h:700,top:scrollMemory.current});
 const dragPreview=useRangeDrag(host,photos,batch&&!filmstrip&&!disabled&&!!onBatchRange,onBatchRange);
 const cols=filmstrip?1:Math.max(1,Math.floor((box.w-32)/210));
 const cell=filmstrip?138:Math.floor((box.w-32)/cols),row=filmstrip?124:Math.round(cell*2/3)+38;
 const count=Math.ceil(photos.length/cols),start=Math.max(0,Math.floor(box.top/row)-1),end=Math.min(count,Math.ceil((box.top+box.h)/row)+1);
 useLayoutEffect(()=>{const el=host.current!;el.scrollTop=scrollMemory.current;const ro=new ResizeObserver(()=>setBox(b=>({...b,w:el.clientWidth,h:el.clientHeight,top:el.scrollTop})));ro.observe(el);return()=>ro.disconnect()},[]);
 useEffect(()=>{if(!filmstrip)return;const i=photos.findIndex(p=>p.id===selected);if(i<0)return;const el=host.current!,top=i*row;if(top<el.scrollTop)el.scrollTop=top;else if(top+row>el.scrollTop+el.clientHeight)el.scrollTop=top+row-el.clientHeight},[selected,filmstrip,row,photos]);
 const children=[];
 for(let r=start;r<end;r++){for(let c=0;c<cols;c++){const i=r*cols+c,p=photos[i];if(!p)break;children.push(<div className="thumb-cell" data-photo-index={i} key={p.id} style={{position:'absolute',top:r*row,left:filmstrip?8:16+c*cell,width:filmstrip?138:cell-14,height:row-8}}><Thumb photo={p} selected={batch?!!checked?.has(p.id)||!!(dragPreview&&i>=dragPreview[0]&&i<=dragPreview[1]):p.id===selected} onClick={()=>onSelect(i)} onDoubleClick={()=>onOpen(i)} batch={batch} onBatchSelect={gesture=>onBatchSelect?.(p.id,gesture)} disabled={disabled}/></div>)}}
 return <div ref={host} className={filmstrip?'filmstrip':'overview'} onScroll={e=>{const top=e.currentTarget.scrollTop;scrollMemory.current=top;setBox(b=>({...b,top}))}} aria-label={filmstrip?'照片缩略图列表':'照片总览'}><div style={{height:count*row+16,position:'relative',marginTop:12}}>{children}</div></div>;
}
