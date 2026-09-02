import React from 'react';
export function Breadcrumb({items=[],className=''}){
  return <nav className={'clr-breadcrumb '+className} aria-label="Breadcrumb">
    {items.map((it,i)=><React.Fragment key={i}>
      {i>0&&<span className="separator">/</span>}
      {i===items.length-1?<span className="current">{it.label}</span>:<a href={it.href||'#'} onClick={it.onClick}>{it.label}</a>}
    </React.Fragment>)}
  </nav>;
}
