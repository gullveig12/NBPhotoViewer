import {useEffect,useLayoutEffect,useRef,useState} from 'react';
export default function ImageContextMenu({x,y,onClose,onCopy,onSave}:{x:number;y:number;onClose:()=>void;onCopy:()=>void;onSave:()=>void}){
 const menu=useRef<HTMLDivElement>(null),[position,setPosition]=useState({left:x,top:y});
 useLayoutEffect(()=>{const r=menu.current!.getBoundingClientRect();setPosition({left:Math.max(8,Math.min(x,innerWidth-r.width-8)),top:Math.max(8,Math.min(y,innerHeight-r.height-8))});menu.current!.querySelector('button')?.focus()},[x,y]);
 useEffect(()=>{const outside=(e:PointerEvent)=>{if(!menu.current?.contains(e.target as Node))onClose()};const resize=()=>onClose();window.addEventListener('pointerdown',outside);window.addEventListener('resize',resize);return()=>{window.removeEventListener('pointerdown',outside);window.removeEventListener('resize',resize)}},[onClose]);
 return <div ref={menu} role="menu" aria-label="照片导出" className="image-context-menu" style={position} onContextMenu={e=>e.preventDefault()} onKeyDown={e=>{
  if(e.key==='Escape'||e.key==='Tab'){e.preventDefault();onClose();return}
  if(e.key==='ArrowDown'||e.key==='ArrowUp'){e.preventDefault();const buttons=Array.from(menu.current!.querySelectorAll('button'));buttons[(buttons.indexOf(document.activeElement as HTMLButtonElement)+(e.key==='ArrowDown'?1:-1)+buttons.length)%buttons.length].focus()}
 }}><button role="menuitem" onClick={onCopy}>复制图片</button><button role="menuitem" onClick={onSave}>将图片另存为 JPEG</button></div>;
}
