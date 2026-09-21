import type {ExportAction,ExportProgress,ExportReport} from './types';
import {demo} from './api';
export interface ExportState {action:ExportAction;jobId:string;progress:ExportProgress;report:ExportReport|null;cancelling:boolean}
export default function ExportPanel({state,onCancel,onClose}:{state:ExportState;onCancel:()=>void;onClose:()=>void}){
 const {action,progress,report,cancelling}=state;
 const title=report?(report.cancelled?'已停止导出':report.fatal?'导出未完成':report.failed.length?'导出完成，部分照片失败':action==='copy'?'已复制图片':action==='save'?'JPEG 已保存':'导出完成'):(action==='copy'?'正在复制图片':action==='save'?'正在保存 JPEG':'正在导出 JPEG');
 return <aside className="export-panel" aria-label="导出进度与结果">
  <div className="export-heading"><strong role="status">{title}{demo?' · 演示':''}</strong>{report?<button onClick={onClose} aria-label="关闭导出结果">关闭</button>:action==='zip'?<button disabled={cancelling} onClick={onCancel}>{cancelling?'正在取消…':'取消导出'}</button>:null}</div>
  {!report?<><p>{cancelling?'正在停止，当前照片处理完成后结束。':`${progress.phase} · ${progress.completed} / ${progress.total} 张`}</p><progress aria-label="导出进度" max={progress.total||1} value={progress.completed}/><p className="export-filename" title={progress.name}>{progress.name}</p></>:<>
   <p>{action==='copy'&&report.exported?'可以粘贴到微信或其他支持图片的应用。':`已导出 ${report.exported} / ${report.total} 张${report.files.length&&action==='zip'?`，共 ${report.files.length} 个 ZIP`:''}。`}</p>
   {report.cancelled&&<p>{report.files.length?'已完成的照片保存在下方 ZIP 中。':'尚未生成 ZIP 文件。'}</p>}
   {report.fatal&&<p role="alert">{report.fatal}</p>}
   {!!report.files.length&&<div className="export-files">{report.files.map(file=><p key={file}>{file}</p>)}</div>}
   {!!report.failed.length&&<details><summary>未导出 {report.failed.length} 张 · 查看原因</summary>{report.failed.map((p,i)=><p key={i}>{p.name}：{p.message}</p>)}</details>}
   {!!report.warnings.length&&<details open><summary>导出说明（{report.warnings.length}）</summary>{report.warnings.map((p,i)=><p key={i}>{p.name}：{p.message}</p>)}</details>}
  </>}
  <p className="muted">JPEG 质量 100 · 不缩小尺寸{action==='zip'?' · 每包小于 1 GB':''}<br/>JPEG 仍为有损格式{demo?' · 演示不写入文件或剪贴板':''}</p>
 </aside>;
}
