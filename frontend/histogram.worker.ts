import {countHistogram,sampleSize} from './histogramMath';
self.onmessage=(event:MessageEvent<{bitmap:ImageBitmap}>)=>{
 const {bitmap}=event.data;
 try{
  const [width,height]=sampleSize(bitmap.width,bitmap.height);
  const canvas=new OffscreenCanvas(width,height);
  const context=canvas.getContext('2d',{willReadFrequently:true,colorSpace:'srgb'});
  if(!context)throw Error('无法读取预览像素');
  context.drawImage(bitmap,0,0,width,height);
  self.postMessage({data:countHistogram(context.getImageData(0,0,width,height).data)});
 }catch(error){self.postMessage({error:error instanceof Error?error.message:String(error)})}
 finally{bitmap.close()}
};
