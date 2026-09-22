export interface Photo { id:string; name:string; path:string; size:number; marked:boolean; isRaw:boolean }
export interface Collection {photos:Photo[]; label:string}
export interface DeleteFailure {id:string;name:string;error:string}
export interface DeleteReport {deleted:string[];failed:DeleteFailure[]}
export interface Metadata {width:number;height:number;flip:number;aperture:number;shutter:number;iso:number;focal:number;timestamp:number;capturedAt:string;model:string;lens:string;rawSupported:boolean}
export type View = 'overview'|'detail';
export type Kind = 'preview'|'full'|'raw';
export type ExportAction='copy'|'save'|'jpeg'|'zip';
export interface ExportProgress {completed:number;total:number;name:string;phase:string}
export interface ExportReport {exported:number;total:number;files:string[];failed:{name:string;message:string}[];warnings:{name:string;message:string}[];cancelled:boolean;fatal:string|null}
