export interface HistogramScale {
 maximum:number;
}
export type HistogramDisplayScale='linear'|'sqrt';

// Keep the original counts and both endpoints. RGB shares a single maximum.
export function histogramScale(channels:readonly Uint32Array[]):HistogramScale {
 let maximum=1;
 for(const channel of channels)for(const count of channel)maximum=Math.max(maximum,count);
 return {maximum};
}

// Change display height only; keep both modes on the same original counts.
export function histogramHeight(count:number,maximum:number,display:HistogramDisplayScale):number {
 const fraction=count/maximum;
 return display==='sqrt'?Math.sqrt(fraction):fraction;
}
