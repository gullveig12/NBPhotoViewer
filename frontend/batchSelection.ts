export interface SelectionGesture {shift?:boolean;additive?:boolean}
export interface BatchSelection {checked:Set<string>;anchor:string|null}
type SelectionAction=
 |({type:'click';id:string;order:readonly string[]}&SelectionGesture)
 |{type:'range';from:string;to:string;order:readonly string[]}
 |{type:'replace';ids:Iterable<string>}
 |{type:'invert';order:readonly string[]}
 |{type:'remove';ids:ReadonlySet<string>}
 |{type:'reset'};
export const emptyBatchSelection=():BatchSelection=>({checked:new Set(),anchor:null});
export function reduceBatchSelection(previous:BatchSelection,action:SelectionAction):BatchSelection{
 if(action.type==='reset')return emptyBatchSelection();
 if(action.type==='replace')return {checked:new Set(action.ids),anchor:null};
 if(action.type==='invert')return {checked:new Set(action.order.filter(id=>!previous.checked.has(id))),anchor:null};
 if(action.type==='remove')return {checked:new Set([...previous.checked].filter(id=>!action.ids.has(id))),anchor:previous.anchor&&action.ids.has(previous.anchor)?null:previous.anchor};
 if(action.type==='range'){
  const from=action.order.indexOf(action.from),to=action.order.indexOf(action.to);
  if(from<0||to<0)return previous;
  const checked=new Set(previous.checked);
  for(let i=Math.min(from,to);i<=Math.max(from,to);i++)checked.add(action.order[i]);
  return {checked,anchor:action.from};
 }
 const target=action.order.indexOf(action.id);
 if(target<0)return previous;
 const anchor=previous.anchor===null?-1:action.order.indexOf(previous.anchor);
 if(action.shift){
  const checked=action.additive?new Set(previous.checked):new Set<string>();
  const start=anchor<0?target:anchor;
  for(let i=Math.min(start,target);i<=Math.max(start,target);i++)checked.add(action.order[i]);
  // Keep the original anchor so repeated Shift clicks can grow/shrink the range.
  return {checked,anchor:anchor<0?action.id:previous.anchor};
 }
 // Ordinary and Ctrl clicks both preserve other selections and toggle this photo.
 const checked=new Set(previous.checked);
 if(checked.has(action.id))checked.delete(action.id);else checked.add(action.id);
 return {checked,anchor:action.id};
}
