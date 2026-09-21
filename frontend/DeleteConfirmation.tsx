import {useEffect,useRef} from 'react';
import type {Photo} from './types';
interface Props {photos:Photo[];batch:boolean;busy:boolean;onCancel:()=>void;onConfirm:()=>void}
export default function DeleteConfirmation({photos,batch,busy,onCancel,onConfirm}:Props){
 const dialog=useRef<HTMLDialogElement>(null),cancel=useRef<HTMLButtonElement>(null);
 useEffect(()=>{
  const previous=document.activeElement as HTMLElement|null;
  dialog.current!.showModal();cancel.current?.focus();
  return()=>{dialog.current?.close();previous?.focus()};
 },[]);
 const marked=photos.filter(p=>p.marked).length;
 return <dialog ref={dialog} className="dialog delete-dialog" aria-labelledby="delete-title" aria-describedby="delete-description" onCancel={e=>{e.preventDefault();if(!busy)onCancel()}}>
  <h2 id="delete-title">{batch?`将 ${photos.length} 张照片移入回收站？`:'移入回收站？'}</h2>
  {!batch&&<p>{photos[0].name}</p>}
  {batch&&<p className="delete-counts">已标记 {marked} 张<span>未标记 {photos.length-marked} 张</span></p>}
  <p className="muted" id="delete-description">照片可从 Windows 回收站恢复。</p>
  <p className="delete-progress" role="status">{busy?`正在处理 ${photos.length} 张照片，请稍候…`:''}</p>
  <div className="dialog-actions"><button ref={cancel} onClick={onCancel} disabled={busy}>取消</button><button className="primary" onClick={onConfirm} disabled={busy}>{busy?'正在处理…':`移入回收站${batch?`（${photos.length}）`:''}`}</button></div>
 </dialog>
}
