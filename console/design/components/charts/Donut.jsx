import React from 'react';
export const VIZ=['var(--cds-alias-viz-general-1)','var(--cds-alias-viz-general-2)','var(--cds-alias-viz-general-3)','var(--cds-alias-viz-general-4)','var(--cds-alias-viz-general-5)','var(--cds-alias-viz-general-6)','var(--cds-alias-viz-general-7)','var(--cds-alias-viz-general-8)'];
export const SEVERITY={success:'var(--cds-alias-viz-severity-success)',warning:'var(--cds-alias-viz-severity-warning)',immediate:'var(--cds-alias-viz-severity-immediate)',critical:'var(--cds-alias-viz-severity-critical)',neutral:'var(--cds-alias-viz-severity-neutral)'};
export function Donut({segments=[],size=96,thickness=12,center,gapDeg=2}){
  const total=segments.reduce((a,s)=>a+s.value,0)||1;
  const r=(size-thickness)/2,c=size/2,circ=2*Math.PI*r;
  let acc=0;
  return <div style={{position:'relative',width:size,height:size,flex:'none'}}>
    <svg width={size} height={size} style={{transform:'rotate(-90deg)'}}>
      <circle cx={c} cy={c} r={r} fill="none" stroke="var(--cds-alias-viz-severity-free-space-fill)" strokeWidth={thickness}/>
      {segments.map((s,i)=>{
        const frac=s.value/total,gap=(gapDeg/360);
        const dash=Math.max(0,(frac-gap))*circ;
        const el=<circle key={i} cx={c} cy={c} r={r} fill="none" stroke={s.color||VIZ[i%VIZ.length]} strokeWidth={thickness}
          strokeDasharray={`${dash} ${circ-dash}`} strokeDashoffset={-acc*circ}/>;
        acc+=frac;return el;})}
    </svg>
    {center&&<div style={{position:'absolute',inset:0,display:'flex',flexDirection:'column',alignItems:'center',justifyContent:'center',gap:0}}>
      <span style={{font:`600 ${Math.round(size/5)}px var(--ns-font-mono)`,color:'var(--cds-alias-typography-color-450)',letterSpacing:'-0.02em'}}>{center.value}</span>
      {center.label&&<span style={{font:'400 10px var(--ns-font-sans)',color:'var(--cds-alias-typography-color-300)'}}>{center.label}</span>}
    </div>}
  </div>;
}
export function ChartLegend({items=[],title,columns=1,valueFormat}){
  const fmt=valueFormat||(v=>typeof v==='number'?v.toLocaleString('en-US').replace(/,/g,'\u2009'):v);
  return <div style={{display:'flex',flexDirection:'column',gap:4,minWidth:0}}>
    {title&&<div style={{font:'600 12px var(--ns-font-sans)',color:'var(--cds-alias-typography-color-450)',marginBottom:2}}>{title}</div>}
    <div style={{display:'grid',gridTemplateColumns:`repeat(${columns},minmax(0,1fr))`,gap:'3px 16px'}}>
      {items.map((it,i)=><div key={i} style={{display:'flex',alignItems:'center',gap:6,font:'400 11px var(--ns-font-sans)',color:'var(--cds-alias-typography-color-300)',whiteSpace:'nowrap',overflow:'hidden',textOverflow:'ellipsis'}}>
        <span style={{width:8,height:8,background:it.color||VIZ[i%VIZ.length],flex:'none'}}></span>
        <span style={{overflow:'hidden',textOverflow:'ellipsis'}}>{it.label}{it.value!=null&&<span style={{fontFamily:'var(--ns-font-mono)'}}>: {fmt(it.value)}</span>}</span>
      </div>)}
    </div>
  </div>;
}
export function ChartStat({label,value}){
  return <div style={{display:'flex',flexDirection:'column',alignItems:'center',gap:2,padding:'0 8px'}}>
    <span style={{font:'400 13px var(--ns-font-sans)',color:'var(--cds-alias-typography-color-450)',whiteSpace:'nowrap'}}>{label}</span>
    <span style={{font:'400 26px var(--ns-font-mono)',color:'var(--cds-alias-typography-color-450)',letterSpacing:'-0.02em'}}>{value}</span>
  </div>;
}
