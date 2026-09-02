import React from 'react';
export function Dropdown({trigger='Actions',variant='outline',sm,items=[],right,className=''}){
  const [open,setOpen]=React.useState(false);
  const ref=React.useRef();
  React.useEffect(()=>{
    const h=e=>{if(ref.current&&!ref.current.contains(e.target))setOpen(false);};
    document.addEventListener('mousedown',h);return ()=>document.removeEventListener('mousedown',h);
  },[]);
  const map={outline:'',primary:'btn-primary',link:'btn-link','link-neutral':'btn-link-neutral',neutral:'btn-neutral'};
  return <div className={'clr-dropdown '+className} ref={ref}>
    <button className={['btn',map[variant]||'',sm?'btn-sm':''].filter(Boolean).join(' ')} onClick={()=>setOpen(o=>!o)} aria-expanded={open}>
      {trigger}<clr-icon shape="angle" dir="down" size="12"></clr-icon>
    </button>
    {open&&<div className={'dropdown-menu'+(right?' right':'')}>
      {items.map((it,i)=>{
        if(it.divider)return <hr key={i} className="dropdown-divider"/>;
        if(it.header)return <div key={i} className="dropdown-header">{it.header}</div>;
        return <button key={i} className={'dropdown-item'+(it.expandable?' expandable':'')} disabled={it.disabled} onClick={()=>{setOpen(false);it.onClick&&it.onClick();}}>
          {it.icon&&<clr-icon shape={it.icon} size="14"></clr-icon>}{it.label}
        </button>;
      })}
    </div>}
  </div>;
}
