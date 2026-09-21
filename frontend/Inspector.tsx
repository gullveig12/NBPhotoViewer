import {memo} from 'react';
import type {Kind,Metadata,Photo} from './types';
import Histogram from './Histogram';
export default memo(function Inspector({photo,meta,bitmap,kind,imageError}:{photo:Photo;meta:Metadata|null;bitmap:ImageBitmap|null;kind:Kind|null;imageError?:string}){
 const shutter=meta?.shutter?(meta.shutter>=1?`${Number(meta.shutter.toFixed(1))} s`:`1/${Math.round(1/meta.shutter)} s`):'—';
 const exposure=[['光圈',meta?.aperture?`f/${Number(meta.aperture.toFixed(1))}`:'—'],['快门',shutter],['ISO',meta?.iso?Math.round(meta.iso):'—'],['焦距',meta?.focal?`${Number(meta.focal.toFixed(1))} mm`:'—']];
 const rows=[['拍摄时间',meta?.capturedAt||(meta?.timestamp?new Date(meta.timestamp*1000).toLocaleString('sv-SE'):'—')],['尺寸',meta?`${meta.width} × ${meta.height}`:'—'],['相机',meta?.model||'—'],['文件大小',`${(photo.size/1048576).toFixed(1)} MB`],['镜头',meta?.lens||'—']];
 return <aside className="inspector" aria-label="照片参数">
  <Histogram photoId={photo.id} bitmap={bitmap} kind={kind} isRaw={photo.isRaw} imageError={imageError}/>
  <dl className="inspector-file"><dt>文件名</dt><dd title={photo.name}>{photo.name}</dd></dl>
  <dl className="exposure-grid">{exposure.map(([name,value])=><div key={name}><dt>{name}</dt><dd>{value}</dd></div>)}</dl>
  <dl className="metadata-list">{rows.map(([name,value])=><div className={'metadata-row'+(name==='镜头'?' lens-row':'')} key={name}><dt>{name}</dt><dd>{value}</dd></div>)}</dl>
 </aside>;
});
