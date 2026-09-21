export interface HistogramData {
 red:Uint32Array; green:Uint32Array; blue:Uint32Array; luminance:Uint32Array; samples:number;
}
// Display-referred sRGB luma, not linear RAW sensor exposure.
export function countHistogram(pixels:Uint8ClampedArray):HistogramData {
 if(pixels.length%4)throw Error('无效的 RGBA 像素数据');
 const red=new Uint32Array(256),green=new Uint32Array(256),blue=new Uint32Array(256),luminance=new Uint32Array(256);
 let samples=0;
 for(let i=0;i<pixels.length;i+=4){
  if(pixels[i+3]===0)continue;
  const r=pixels[i],g=pixels[i+1],b=pixels[i+2];
  red[r]++;green[g]++;blue[b]++;
  luminance[Math.round((2126*r+7152*g+722*b)/10000)]++;samples++;
 }
 return {red,green,blue,luminance,samples};
}
export function sampleSize(width:number,height:number):[number,number]{
 if(!Number.isFinite(width)||!Number.isFinite(height)||width<=0||height<=0)throw Error('图像尺寸无效');
 const scale=Math.min(1,1024/Math.max(width,height));
 return [Math.max(1,Math.round(width*scale)),Math.max(1,Math.round(height*scale))];
}
