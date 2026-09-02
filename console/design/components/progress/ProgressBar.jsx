import React from 'react';
export function ProgressBar({value=0,max=100,status,loop,sm,label,showValue,className=''}){
  const cls=['progress',status||'',loop?'loop':'',sm?'progress-sm':'',className].filter(Boolean).join(' ');
  const bar=<span className={cls}><span className="progress-fill" style={loop?undefined:{width:(100*value/max)+'%'}}></span></span>;
  if(!label&&!showValue)return bar;
  return <span className="progress-block">{label&&<span className="progress-label">{label}</span>}{bar}{showValue&&<span className="progress-value">{Math.round(100*value/max)}%</span>}</span>;
}
export function Spinner({size='lg',inline,inverse,className='',...rest}){
  const cls=['spinner',size==='md'?'spinner-md':'',size==='sm'?'spinner-sm':'',inline?'spinner-inline':'',inverse?'spinner-inverse':'',className].filter(Boolean).join(' ');
  return <span className={cls} role="progressbar" {...rest}></span>;
}
export function Skeleton({width='100%',height=12,className='',style}){
  return <span className={'skeleton '+className} style={{width,height,...style}}></span>;
}
