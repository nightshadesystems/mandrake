import React from 'react';
export function Signpost({title,children,className=''}){
  const [open,setOpen]=React.useState(false);
  const ref=React.useRef();
  React.useEffect(()=>{
    const h=e=>{if(ref.current&&!ref.current.contains(e.target))setOpen(false);};
    document.addEventListener('mousedown',h);return ()=>document.removeEventListener('mousedown',h);
  },[]);
  return <span ref={ref} className={className} style={{position:'relative',display:'inline-flex'}}>
    <button className={'signpost-trigger'+(open?' active':'')} aria-label="More info" onClick={()=>setOpen(o=>!o)}><clr-icon shape="info-circle" size="16" class={open?'is-solid':''}></clr-icon></button>
    {open&&<span className="signpost-content" style={{top:'calc(100% + 8px)',left:0}}>
      {title&&<span className="signpost-title">{title}<button className="close" onClick={()=>setOpen(false)}>×</button></span>}
      {children}
    </span>}
  </span>;
}
