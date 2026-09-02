import React from 'react';
const DAYS=['M','T','W','T','F','S','S'];
const MONTHS=['January','February','March','April','May','June','July','August','September','October','November','December'];
function fmt(d){return d?`${String(d.getMonth()+1).padStart(2,'0')}/${String(d.getDate()).padStart(2,'0')}/${d.getFullYear()}`:'';}
export function DatePicker({defaultValue,onChange,placeholder='MM/DD/YYYY',className=''}){
  const [open,setOpen]=React.useState(false);
  const [val,setVal]=React.useState(defaultValue?new Date(defaultValue):null);
  const [view,setView]=React.useState(()=>val||new Date());
  const ref=React.useRef();
  React.useEffect(()=>{
    const h=e=>{if(ref.current&&!ref.current.contains(e.target))setOpen(false);};
    document.addEventListener('mousedown',h);return ()=>document.removeEventListener('mousedown',h);
  },[]);
  const y=view.getFullYear(),m=view.getMonth();
  const first=(new Date(y,m,1).getDay()+6)%7;
  const nDays=new Date(y,m+1,0).getDate();
  const cells=[...Array(first).fill(null),...Array(nDays).keys()].map(v=>v===null?null:v+1);
  const today=new Date();
  const pick=d=>{const nd=new Date(y,m,d);setVal(nd);onChange&&onChange(nd);setOpen(false);};
  const nav=off=>setView(new Date(y,m+off,1));
  const cellStyle={all:'unset',width:28,height:28,display:'inline-flex',alignItems:'center',justifyContent:'center',borderRadius:4,fontSize:12,fontFamily:'var(--ns-font-mono)',cursor:'pointer',color:'var(--cds-alias-typography-color-400)'};
  return <div ref={ref} className={'clr-dropdown '+className}>
    <div className="clr-input-group" style={{width:180}}>
      <input className="clr-input" value={fmt(val)} placeholder={placeholder} readOnly onClick={()=>setOpen(o=>!o)} style={{cursor:'pointer'}}/>
      <button className="clr-input-group-addon" style={{cursor:'pointer',background:'var(--ns-input-bg)'}} onClick={()=>setOpen(o=>!o)} aria-label="Open calendar"><clr-icon shape="calendar" size="14"></clr-icon></button>
    </div>
    {open&&<div className="dropdown-menu" style={{padding:12,minWidth:0,width:236}}>
      <div style={{display:'flex',alignItems:'center',justifyContent:'space-between',marginBottom:8}}>
        <button style={cellStyle} onClick={()=>nav(-1)} aria-label="Previous month"><clr-icon shape="angle" dir="left" size="12"></clr-icon></button>
        <span style={{fontSize:13,fontWeight:600,color:'var(--cds-alias-typography-color-450)'}}>{MONTHS[m]} {y}</span>
        <button style={cellStyle} onClick={()=>nav(1)} aria-label="Next month"><clr-icon shape="angle" dir="right" size="12"></clr-icon></button>
      </div>
      <div style={{display:'grid',gridTemplateColumns:'repeat(7,28px)',gap:2}}>
        {DAYS.map((d,i)=><span key={i} style={{...cellStyle,cursor:'default',color:'var(--cds-alias-typography-color-200)',fontSize:10,fontWeight:600}}>{d}</span>)}
        {cells.map((d,i)=>d===null?<span key={i}></span>:
          <button key={i} style={{...cellStyle,
            background:val&&val.getDate()===d&&val.getMonth()===m&&val.getFullYear()===y?'var(--ns-accent)':'transparent',
            color:val&&val.getDate()===d&&val.getMonth()===m&&val.getFullYear()===y?'var(--cds-alias-interaction-on-action)':(today.getDate()===d&&today.getMonth()===m&&today.getFullYear()===y?'var(--ns-accent)':cellStyle.color),
            fontWeight:today.getDate()===d&&today.getMonth()===m&&today.getFullYear()===y?700:400}}
            onClick={()=>pick(d)} onMouseEnter={e=>{if(!(val&&val.getDate()===d&&val.getMonth()===m))e.target.style.background='var(--cds-alias-object-interaction-background-hover)';}} onMouseLeave={e=>{if(!(val&&val.getDate()===d&&val.getMonth()===m&&val.getFullYear()===y))e.target.style.background='transparent';}}>{d}</button>)}
      </div>
    </div>}
  </div>;
}
