import {useCallback,useEffect,useMemo,useReducer,useRef,useState} from 'react';
import {choose,demo,markPhoto,metadata,qa,trashPhotos,exportDestination,exportPhotos,cancelExport} from './api';
import {cacheStats,loadImage,peek,pin,resetImages} from './images';
import type {Collection,DeleteReport,Kind,Metadata,Photo,View,ExportAction} from './types';
import Thumbnails from './Thumbnails';
import Viewer,{type ViewerHandle} from './Viewer';
import Inspector from './Inspector';
import DeleteConfirmation from './DeleteConfirmation';
import {resetHistograms} from './histograms';
import ImageContextMenu from './ImageContextMenu';
import ExportPanel,{type ExportState} from './ExportPanel';
import {emptyBatchSelection,reduceBatchSelection,type SelectionGesture} from './batchSelection';

type DeletePrompt={photos:Photo[];batch:boolean};
export default function App(){
 const [collection,setCollection]=useState<Collection>({photos:[],label:''});
 const [index,setIndex]=useState(0),[view,setView]=useState<View>('overview');
 const [bitmap,setBitmap]=useState<ImageBitmap|null>(null),[kind,setKind]=useState<Kind|null>(null),[meta,setMeta]=useState<Metadata|null>(null);
 const [preferRaw,setPreferRaw]=useState(false);
 const [imageError,setImageError]=useState('');
 const [bitmapPhotoId,setBitmapPhotoId]=useState('');
 const [zoom,setZoom]=useState(100),[sourceBusy,setBusy]=useState(false),[message,setMessage]=useState(''),[rawError,setRawError]=useState('');
 const [exportBusy,setExportBusy]=useState(false),[exportState,setExportState]=useState<ExportState|null>(null);
 const [contextMenu,setContextMenu]=useState<{x:number;y:number;photo:Photo;preferRaw:boolean}|null>(null);
 const busy=sourceBusy||exportBusy;
 const [deletePrompt,setDeletePrompt]=useState<DeletePrompt|null>(null),[deleting,setDeleting]=useState(false),[deleteReport,setDeleteReport]=useState<DeleteReport|null>(null);
 const [elapsed,setElapsed]=useState<number|null>(null),[markedOnly,setMarkedOnly]=useState(false);
 const [batch,setBatch]=useState(false);
 const [batchSelection,dispatchSelection]=useReducer(reduceBatchSelection,undefined,emptyBatchSelection);
 const checked=batchSelection.checked;
 const photoOrder=useMemo(()=>collection.photos.map(p=>p.id),[collection.photos]);
 const viewer=useRef<ViewerHandle>(null),overviewScroll=useRef(0),filmScroll=useRef(0),request=useRef(0),fileBusy=useRef(false),sequence=useRef(0);
 const photos=useMemo(()=>markedOnly&&!batch?collection.photos.filter(p=>p.marked):collection.photos,[collection.photos,markedOnly,batch]);
 const selected=photos[Math.min(index,Math.max(0,photos.length-1))],id=selected?.id||'';
 const smallPreview=selected?.isRaw&&kind==='full'&&bitmapPhotoId===id&&bitmap&&meta&&bitmap.width*bitmap.height<meta.width*meta.height*.8;
 const previewNotice=smallPreview?`当前预览 ${bitmap!.width} × ${bitmap!.height}；${meta?.rawSupported?'检查对焦可切换 RAW 显影。':'此文件暂不支持 RAW 显影。'}`:'';
 const chosenPhotos=useMemo(()=>collection.photos.filter(p=>checked.has(p.id)),[collection.photos,checked]);
 const markedCount=useMemo(()=>collection.photos.filter(p=>p.marked).length,[collection.photos]);
 useEffect(()=>setIndex(i=>Math.min(i,Math.max(0,photos.length-1))),[photos.length]);
 const metrics=useRef<{id:string;kind:Kind;ms:number;cached:boolean}[]>([]);
 const select=useCallback((i:number)=>setIndex(Math.max(0,i)),[]);
 const open=useCallback((i:number)=>{setIndex(i);setView('detail')},[]);
 const error=(e:unknown)=>setMessage(e instanceof Error?e.message:String(e));
 const exitBatch=()=>{if(fileBusy.current)return;setBatch(false);dispatchSelection({type:'reset'})};
 const enterBatch=()=>{
  setIndex(Math.max(0,collection.photos.findIndex(p=>p.id===id)));
  setMarkedOnly(false);setBatch(true);dispatchSelection({type:'reset'});setDeleteReport(null);setMessage('');
 };
 const selectBatch=useCallback((photoId:string,gesture:SelectionGesture)=>{
  if(fileBusy.current||deletePrompt)return;
  dispatchSelection({type:'click',id:photoId,order:photoOrder,...gesture});
 },[photoOrder,deletePrompt]);
 const selectBatchRange=useCallback((from:string,to:string)=>{
  if(fileBusy.current||deletePrompt)return;
  dispatchSelection({type:'range',from,to,order:photoOrder});
 },[photoOrder,deletePrompt]);
 const chooseSource=async(folder:boolean)=>{
  if(fileBusy.current)return;fileBusy.current=true;setBusy(true);setMessage('');
  try{
   const next=await choose(folder);
   if(next){
    sequence.current++;request.current++;setBitmap(null);setBitmapPhotoId('');resetImages();resetHistograms();setCollection(next);setIndex(0);
    setMeta(null);setKind(null);setRawError('');setMarkedOnly(false);setBatch(false);dispatchSelection({type:'reset'});setDeleteReport(null);
    overviewScroll.current=0;filmScroll.current=0;
    if(!next.photos.length)setMessage('所选位置没有支持的照片。支持常见图片和相机 RAW。');
   }
  }catch(e){error(e)}finally{fileBusy.current=false;setBusy(false)}
 };
 const mark=async()=>{
  if(!selected||fileBusy.current||batch)return;fileBusy.current=true;const target=selected;
  try{await markPhoto(target.id,!target.marked);setCollection(c=>({...c,photos:c.photos.map(p=>p.id===target.id?{...p,marked:!p.marked}:p)}))}
  catch(e){error(e)}finally{fileBusy.current=false}
 };
 const askDelete=()=>{
  if(fileBusy.current)return;
  const targets=batch?chosenPhotos:selected?[selected]:[];
  if(targets.length)setDeletePrompt({photos:targets.slice(),batch});
 };
 const remove=async()=>{
  if(!deletePrompt||fileBusy.current)return;
  fileBusy.current=true;setDeleting(true);setMessage('');
  const prompt=deletePrompt;
  try{
   const result=await trashPhotos(prompt.photos.map(p=>p.id));
   const removed=new Set(result.deleted);
   setCollection(c=>({...c,photos:c.photos.filter(p=>!removed.has(p.id))}));
   dispatchSelection({type:'remove',ids:removed});
   setDeleteReport(result);setDeletePrompt(null);
   if(result.deleted.length===collection.photos.length){setBatch(false);dispatchSelection({type:'reset'})}
  }catch(e){error(e);setDeletePrompt(null)}finally{fileBusy.current=false;setDeleting(false)}
 };
 const closeContextMenu=useCallback(()=>{setContextMenu(null);document.querySelector<HTMLElement>('.viewer')?.focus({preventScroll:true})},[]);
 useEffect(()=>setContextMenu(null),[id,view]);
 const startExport=async(action:ExportAction,targets:Photo[],raw=false)=>{
  closeContextMenu();if(fileBusy.current||!targets.length)return;
  fileBusy.current=true;setExportBusy(true);setMessage('');setExportState(null);
  const jobId=crypto.randomUUID();
  try{
   const path=await exportDestination(action,targets[0].name);if(path===null)return;
   setExportState({action,jobId,progress:{completed:0,total:targets.length,name:targets[0].name,phase:'正在转换 JPEG'},report:null,cancelling:false});
   const report=await exportPhotos(action,targets,raw,path,jobId,progress=>setExportState(s=>s?.jobId===jobId?{...s,progress}:s));
   setExportState(s=>s?.jobId===jobId?{...s,report}:s);
  }catch(e){const message=e instanceof Error?e.message:String(e);setExportState({action,jobId,progress:{completed:0,total:targets.length,name:'',phase:''},report:{exported:0,total:targets.length,files:[],failed:[],warnings:[],cancelled:false,fatal:message},cancelling:false})}
  finally{fileBusy.current=false;setExportBusy(false)}
 };
 const stopExport=async()=>{
  if(!exportState||exportState.report)return;const jobId=exportState.jobId;
  setExportState(s=>s?{...s,cancelling:true}:s);
  try{await cancelExport(jobId)}catch(e){error(e);setExportState(s=>s?.jobId===jobId?{...s,cancelling:false}:s)}
 };
 useEffect(()=>{
  if(!id){setBitmap(null);setMeta(null);setKind(null);return}
  // Overview only needs thumbnails. Full images and neighbour preloads compete
  // with the first grid for CPU/memory and begin when detail view is opened.
  if(view!=='detail')return;
  const token=++request.current;const start=performance.now();let disposed=false;setRawError('');setImageError('');setElapsed(null);pin(id);
  let rawReady=false;
  const show=(b:ImageBitmap,k:Kind,cached:boolean)=>{
   if(disposed||token!==request.current||!b.width)return;if(k!=='raw'&&rawReady)return;if(k==='raw')rawReady=true;
   setBitmap(b);setBitmapPhotoId(id);setKind(k);setElapsed(Math.round(performance.now()-start));
   metrics.current.push({id,kind:k,ms:Math.round(performance.now()-start),cached});if(metrics.current.length>200)metrics.current.shift();
  };
  const cachedRaw=preferRaw?peek(id,'raw'):null,cachedFull=peek(id,'full');
  if(cachedRaw)show(cachedRaw,'raw',true);else if(cachedFull)show(cachedFull,'full',true);else{setBitmap(null);setKind(null)}
  setMeta(null);
  const info=metadata(id).then(m=>{if(!disposed&&token===request.current)setMeta(m);return m});
  info.catch(e=>{if(!disposed)error(e)});
  const full=cachedFull?Promise.resolve(cachedFull):loadImage(id,'full');
  full.then(b=>show(b,'full',!!cachedFull)).catch(e=>{if(!disposed){setImageError('无法显示这张照片，可继续浏览其他照片。');error(e)}});
  let timer=0;
  if(view==='detail'&&preferRaw&&selected?.isRaw)timer=window.setTimeout(async()=>{
   try{const m=await info;if(disposed)return;if(!m.rawSupported){setRawError('此 RAW 暂不支持显影，当前显示可用的内嵌预览。');return}show(await loadImage(id,'raw'),'raw',!!cachedRaw)}
   catch(e){if(!disposed)setRawError(e instanceof Error?e.message:String(e))}
  },60);
  const previewTimer=window.setTimeout(async()=>{
   try{await full}catch{}
   for(const offset of [1,2,-1]){
    if(disposed||token!==request.current)break;
    const p=photos[index+offset];if(!p)continue;
    try{await loadImage(p.id,'full');if(preferRaw&&p.isRaw&&!disposed){const m=await metadata(p.id);if(m.rawSupported&&!disposed)await loadImage(p.id,'raw')}}catch{}
   }
  },80);
  return()=>{disposed=true;clearTimeout(timer);clearTimeout(previewTimer)};
 },[id,view,collection.label,preferRaw]);
 useEffect(()=>{
  const key=(e:KeyboardEvent)=>{
   const target=e.target as HTMLElement;
   if(target.closest('dialog,[role="menu"],.export-panel')||contextMenu||deletePrompt||deleting||busy||e.altKey||e.ctrlKey||e.metaKey)return;
   if(target instanceof HTMLTextAreaElement||(target instanceof HTMLInputElement&&target.type!=='checkbox'))return;
   if(batch){
    if(e.key==='Delete'){e.preventDefault();askDelete()}
    else if(e.key==='Escape'){e.preventDefault();exitBatch()}
    return;
   }
   if(e.key==='ArrowRight'||e.key==='ArrowDown'){e.preventDefault();setIndex(i=>Math.max(0,Math.min(photos.length-1,i+1)))}
   else if(e.key==='ArrowLeft'||e.key==='ArrowUp'){e.preventDefault();setIndex(i=>Math.max(0,i-1))}
   else if(e.key.toLowerCase()==='f'){e.preventDefault();void mark()}
   else if(e.key==='Enter'&&!target.closest('button')){e.preventDefault();setView(v=>v==='detail'?'overview':'detail')}
   else if(e.key==='Delete'&&selected){e.preventDefault();askDelete()}
   else if(e.key===' '&&view==='detail'&&!target.closest('button')){e.preventDefault();zoom===100?viewer.current?.fit():viewer.current?.actual()}
  };
  window.addEventListener('keydown',key);return()=>window.removeEventListener('keydown',key);
 },[photos.length,selected,view,zoom,deletePrompt,deleting,busy,batch,chosenPhotos,contextMenu]);
 useEffect(()=>{if(qa)Object.assign(window,{__NBPHOTO_QA__:{getMetrics:()=>metrics.current,getCache:cacheStats}})},[]);
 return <div className="app">
  <header className="toolbar">
   <div className="switch" role="group" aria-label="视图"><button className={view==='overview'?'active':''} disabled={busy||deleting} onClick={()=>setView('overview')}>总览</button><button className={view==='detail'?'active':''} disabled={!selected||batch||busy||deleting} onClick={()=>setView('detail')}>详细</button></div>
   <span className="divider"/><button onClick={()=>void chooseSource(true)} disabled={busy||deleting}>选择文件夹</button><button onClick={()=>void chooseSource(false)} disabled={busy||deleting}>选择照片</button>
   <span className="collection-count" title={collection.label}>{sourceBusy?'正在读取…':`${photos.length} 张`}</span>
   <div className="toolbar-spacer"/>
   {batch?<><span className="batch-mode-label">批量选择</span><button onClick={exitBatch} disabled={deleting||busy}>完成</button></>:<>
    {!!collection.photos.length&&<button className={'filter'+(markedOnly?' active':'')} disabled={busy} onClick={()=>{setMarkedOnly(v=>!v);setIndex(0)}} title="只显示已标记照片">已标记 {markedCount}</button>}
    {view==='overview'&&!!collection.photos.length&&<button onClick={enterBatch} disabled={busy}>批量选择</button>}
    <button disabled={!selected||busy} onClick={()=>void mark()} title="F · 标记／取消标记">{selected?.marked?'取消标记':'标记'}</button><button disabled={!selected||busy} onClick={askDelete} title="Delete · 移入回收站">删除</button>
   </>}
  </header>
  {batch&&<div className="batch-toolbar" role="region" aria-label="批量选择操作">
   <button disabled={busy||deleting||markedCount===0} onClick={()=>dispatchSelection({type:'replace',ids:collection.photos.filter(p=>p.marked).map(p=>p.id)})}>选择已标记</button>
   <button disabled={busy||deleting} onClick={()=>dispatchSelection({type:'invert',order:photoOrder})}>反选</button>
   <button disabled={busy||deleting||!chosenPhotos.length} onClick={()=>dispatchSelection({type:'reset'})}>清空选择</button>
   <span className="divider"/><span className="batch-scope" title="单击追加或取消；按住拖动连选，松开确认，Esc 取消；Shift 连选；Ctrl 追加或取消">范围：本次全部照片</span><div className="toolbar-spacer"/>
   <span className="batch-count" role="status" aria-live="polite">已选 {chosenPhotos.length} 张</span>
   <div className="batch-actions" role="group" aria-label="所选照片操作">
    <button disabled={busy||deleting||!chosenPhotos.length} onClick={()=>void startExport('zip',chosenPhotos.slice())}>导出（{chosenPhotos.length}）</button>
    <button className="primary" disabled={busy||deleting||!chosenPhotos.length} onClick={askDelete}>删除（{chosenPhotos.length}）</button>
   </div>
  </div>}
  {message&&<div className="notice" role="alert"><span>{message}</span><button onClick={()=>setMessage('')} aria-label="关闭提示">关闭</button></div>}
  {deleteReport&&<div className="notice delete-result" role="status"><div>
   <p>已移入回收站 {deleteReport.deleted.length} 张{deleteReport.failed.length?`，${deleteReport.failed.length} 张未删除，已保留。`:'。'}</p>
   {!!deleteReport.failed.length&&<details><summary>查看未删除原因</summary><ul>{deleteReport.failed.map(p=><li key={p.id}><strong>{p.name}</strong>：{p.error}</li>)}</ul></details>}
  </div><button onClick={()=>setDeleteReport(null)} aria-label="关闭删除结果">关闭</button></div>}
  {!collection.photos.length?<main className="empty"><div><h1>选择要浏览的照片</h1><p>打开一个文件夹，或选择多张图片和 RAW 照片。</p><div className="empty-actions"><button onClick={()=>void chooseSource(true)} disabled={busy}>选择文件夹</button><button onClick={()=>void chooseSource(false)} disabled={busy}>选择照片</button></div>{qa&&<p className="qa-hint">{demo?'交互演示 · 不修改原片':'浏览器验证模式 · 原片只读'}</p>}</div></main>:
   !photos.length?<main className="empty"><div><p>还没有已标记的照片。</p><button onClick={()=>setMarkedOnly(false)}>显示全部照片</button></div></main>:
   view==='overview'?<Thumbnails key={'overview'+sequence.current} photos={photos} selected={id} onSelect={select} onOpen={open} scrollMemory={overviewScroll} batch={batch} checked={checked} onBatchSelect={selectBatch} onBatchRange={selectBatchRange} disabled={busy||deleting||!!deletePrompt}/>:
   <main className="detail"><Thumbnails key={'film'+sequence.current} photos={photos} selected={id} filmstrip onSelect={select} onOpen={open} scrollMemory={filmScroll}/><section className="image-panel"><div className="image-tools"><button onClick={()=>setPreferRaw(v=>!v)} title={!selected.isRaw?'显示原图；TIFF 使用无损预览':meta?.rawSupported?'切换 RAW 预览与显影；预览优先使用相机内嵌图，缺失时尝试显影':'使用可用的内嵌预览；此 RAW 暂不支持显影'} disabled={!selected.isRaw||!meta?.rawSupported}>{!selected.isRaw?'原图':preferRaw&&meta?.rawSupported?'RAW 显影':'RAW 预览'}</button><button onClick={()=>viewer.current?.fit()}>适应窗口</button><button onClick={()=>viewer.current?.actual()}>100%</button></div><Viewer ref={viewer} bitmap={bitmap} photoId={id} meta={meta} kind={kind} error={imageError} onZoom={setZoom} onContextMenu={e=>{e.preventDefault();if(selected&&!fileBusy.current&&!deletePrompt)setContextMenu({x:e.clientX,y:e.clientY,photo:selected,preferRaw})}}/>{(rawError||previewNotice)&&<div className="raw-warning" role="status">{rawError||previewNotice}</div>}</section><Inspector photo={selected} meta={meta} imageError={imageError} bitmap={bitmapPhotoId===id?bitmap:null} kind={bitmapPhotoId===id?kind:null}/></main>}
  <footer className="statusbar"><span title={selected?.path}>{demo?'交互演示 · 原片只读':selected?.name||'NBPhotoViewer'}</span><span className="navigation">
   {batch?`${collection.photos.length} 张`:<>{!!selected&&<button aria-label="上一张" title="← 上一张" disabled={index===0} onClick={()=>setIndex(i=>Math.max(0,i-1))}>‹</button>}{selected?`${Math.min(index+1,photos.length)} / ${photos.length} 张`:''}{!!selected&&<button aria-label="下一张" title="→ 下一张" disabled={index>=photos.length-1} onClick={()=>setIndex(i=>Math.min(photos.length-1,i+1))}>›</button>}</>}
  </span><span>{batch?`已选 ${chosenPhotos.length} 张`:view==='detail'&&selected?`${zoom}%　 |　 ${kind==='raw'?'RAW 就绪':kind==='full'?(!selected.isRaw?'原图':preferRaw&&meta?.rawSupported&&!rawError?'RAW 预览 · 显影中':'RAW 预览'):imageError?'读取失败':'正在读取'}${qa&&elapsed!==null?' · '+elapsed+' ms':''}`:''}</span></footer>
  {deletePrompt&&<DeleteConfirmation photos={deletePrompt.photos} batch={deletePrompt.batch} busy={deleting} onCancel={()=>!deleting&&setDeletePrompt(null)} onConfirm={()=>void remove()}/>}
  {contextMenu&&<ImageContextMenu x={contextMenu.x} y={contextMenu.y} onClose={closeContextMenu} onCopy={()=>void startExport('copy',[contextMenu.photo],contextMenu.preferRaw)} onSave={()=>void startExport('save',[contextMenu.photo],contextMenu.preferRaw)}/>}
  {exportState&&<ExportPanel state={exportState} onCancel={()=>void stopExport()} onClose={()=>setExportState(null)}/>}
 </div>
}
