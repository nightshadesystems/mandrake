import React from 'react';
export function Combobox({options=[],multi,placeholder='Select…',defaultValue,onChange,className=''}){
  const [open,setOpen]=React.useState(false);
  const [q,setQ]=React.useState('');
  const [val,setVal]=React.useState(multi?(defaultValue||[]):(defaultValue||null));
  const ref=React.useRef();
  React.useEffect(()=>{
    const h=e=>{if(ref.current&&!ref.current.contains(e.target))setOpen(false);};
    document.addEventListener('mousedown',h);return ()=>document.removeEventListener('mousedown',h);
  },[]);
  const filtered=options.filter(o=>o.toLowerCase().includes(q.toLowerCase()));
  const pick=o=>{
    if(multi){const n=val.includes(o)?val.filter(x=>x!==o):[...val,o];setVal(n);onChange&&onChange(n);setQ('');}
    else{setVal(o);onChange&&onChange(o);setQ('');setOpen(false);}
  };
  return <div ref={ref} className={'clr-dropdown '+className} style={{display:'block',maxWidth:320}}>
    <div className="clr-input" style={{display:'flex',alignItems:'center',gap:4,height:'auto',minHeight:32,flexWrap:'wrap',padding:'3px 28px 3px 6px',position:'relative',cursor:'text'}} onClick={()=>setOpen(true)}>
      {multi&&val.map(v=><span key={v} className="label">{v}<button className="label-dismiss" onClick={e=>{e.stopPropagation();pick(v);}}>×</button></span>)}
      <input value={q} placeholder={multi?(val.length?'':placeholder):(val||placeholder)} onChange={e=>{setQ(e.target.value);setOpen(true);}}
        style={{all:'unset',flex:1,minWidth:60,font:'inherit',color:'var(--cds-alias-typography-color-450)'}}/>
      <clr-icon shape="angle" dir={open?'up':'down'} size="12" style={{position:'absolute',right:8,color:'var(--cds-alias-object-interaction-color)'}}></clr-icon>
    </div>
    {open&&<div className="dropdown-menu" style={{width:'100%'}}>
      {filtered.length===0&&<div className="dropdown-header">No matches</div>}
      {filtered.map(o=><button key={o} className="dropdown-item" onClick={()=>pick(o)}>
        {multi&&<clr-icon shape="check" size="12" style={{visibility:val.includes(o)?'visible':'hidden',color:'var(--ns-accent)'}}></clr-icon>}
        {!multi&&val===o&&<clr-icon shape="check" size="12" style={{color:'var(--ns-accent)'}}></clr-icon>}
        {o}
      </button>)}
    </div>}
  </div>;
}
