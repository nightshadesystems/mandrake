import React from 'react';
import {VIZ} from './Donut.jsx';
export function LineChart({series=[],labels=[],height=160,area=true,yFormat,className=''}){
  const w=100,h=100;
  const all=series.flatMap(s=>s.points);
  const max=Math.max(...all,1),min=Math.min(...all,0);
  const n=Math.max(...series.map(s=>s.points.length),2);
  const x=i=>i*(w/(n-1));
  const y=v=>h-((v-min)/(max-min||1))*h;
  const fmt=yFormat||(v=>String(v));
  // Quantize ticks to a "nice" step so caller-supplied formatters never see float noise.
  const niceStep=r=>{if(!(r>0))return 1;const mag=Math.pow(10,Math.floor(Math.log10(r)));const f=r/mag;return (f<1.5?1:f<3?2:f<7?5:10)*mag;};
  const step=niceStep((max-min)/4);
  const dec=Math.max(0,Math.min(6,-Math.floor(Math.log10(step))+(step<1?0:0)));
  const round=v=>Number(v.toFixed(dec));
  const ticks=[0,0.25,0.5,0.75,1].map(t=>round(min+t*(max-min)));
  return <div className={className} style={{display:'flex',gap:8}}>
    <div style={{display:'flex',flexDirection:'column',justifyContent:'space-between',textAlign:'right',font:'400 9px var(--ns-font-mono)',color:'var(--cds-alias-typography-color-200)',paddingBottom:16}}>
      {[...ticks].reverse().map((t,i)=><span key={i}>{fmt(t)}</span>)}
    </div>
    <div style={{flex:1,minWidth:0,display:'flex',flexDirection:'column',gap:4}}>
      <svg viewBox={`0 0 ${w} ${h}`} preserveAspectRatio="none" style={{width:'100%',height}}>
        {ticks.map((t,i)=><line key={i} x1="0" x2={w} y1={y(t)} y2={y(t)} stroke="var(--cds-alias-object-border-subtle)" strokeWidth="0.4"/>)}
        {series.map((s,si)=>{
          const col=s.color||VIZ[si%VIZ.length];
          const pts=s.points.map((v,i)=>`${x(i)},${y(v)}`).join(' ');
          return <g key={si}>
            {area&&<polygon points={`0,${h} ${pts} ${x(s.points.length-1)},${h}`} fill={col} opacity="0.12"/>}
            <polyline points={pts} fill="none" stroke={col} strokeWidth="1.5" vectorEffect="non-scaling-stroke"/>
          </g>;})}
      </svg>
      {labels.length>0&&<div style={{display:'flex',justifyContent:'space-between',font:'400 9px var(--ns-font-mono)',color:'var(--cds-alias-typography-color-200)'}}>{labels.map((l,i)=><span key={i}>{l}</span>)}</div>}
    </div>
  </div>;
}
