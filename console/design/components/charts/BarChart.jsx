import React from 'react';
import {VIZ} from './Donut.jsx';
export function BarChart({items=[],max,valueFormat,axisLabel,className=''}){
  const m=max||Math.max(...items.map(it=>(it.segments||[{value:it.value}]).reduce((a,s)=>a+s.value,0)),1);
  const fmt=valueFormat||(v=>v.toLocaleString('en-US').replace(/,/g,' '));
  return <div className={className} style={{display:'flex',gap:10}}>
    {axisLabel&&<div style={{writingMode:'vertical-rl',transform:'rotate(180deg)',font:'400 10px var(--ns-font-sans)',color:'var(--cds-alias-typography-color-200)',textAlign:'center'}}>{axisLabel}</div>}
    <div style={{display:'flex',flexDirection:'column',gap:12,flex:1,minWidth:0}}>
      {items.map((it,i)=>{
        const segs=it.segments||[{value:it.value,color:it.color}];
        const total=segs.reduce((a,s)=>a+s.value,0);
        return <div key={i} style={{display:'flex',flexDirection:'column',gap:3}}>
          <div style={{display:'flex',alignItems:'center',height:16}}>
            {segs.map((s,si)=><span key={si} style={{width:(100*s.value/m)+'%',minWidth:s.value>0?2:0,height:'100%',background:s.color||VIZ[si%VIZ.length]}}></span>)}
            <span style={{font:'400 10px var(--ns-font-mono)',color:'var(--cds-alias-typography-color-300)',marginLeft:6,whiteSpace:'nowrap'}}>{fmt(total)}</span>
          </div>
          <a href="#" onClick={e=>{e.preventDefault();it.onClick&&it.onClick();}} style={{font:'400 11px var(--ns-font-sans)',width:'fit-content'}}>{it.label}</a>
        </div>;})}
    </div>
  </div>;
}
