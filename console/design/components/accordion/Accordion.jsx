import React from 'react';
export function Accordion({panels=[],multi,defaultOpen=[],className=''}){
  const [open,setOpen]=React.useState(()=>new Set(defaultOpen));
  const toggle=i=>setOpen(s=>{const n=new Set(multi?s:[]);if(s.has(i)){n.delete(i);}else{n.add(i);}return n;});
  return <div className={'clr-accordion '+className}>
    {panels.map((p,i)=><div key={i} className={'clr-accordion-panel'+(open.has(i)?' open':'')}>
      <button className="clr-accordion-header" aria-expanded={open.has(i)} onClick={()=>toggle(i)}>
        <span className="clr-accordion-caret"><clr-icon shape="angle" dir="right" size="10"></clr-icon></span>
        <span>{p.title}</span>
        {p.description&&<span className="clr-accordion-description">{p.description}</span>}
      </button>
      {open.has(i)&&<div className="clr-accordion-content">{p.content}</div>}
    </div>)}
  </div>;
}
export function CollapsiblePanel({title,defaultOpen,children,className=''}){
  const [open,setOpen]=React.useState(!!defaultOpen);
  return <div className={['clr-collapsible-panel','clr-accordion-panel',open?'open':'',className].filter(Boolean).join(' ')}>
    <button className="clr-accordion-header" aria-expanded={open} onClick={()=>setOpen(o=>!o)}>
      <span className="clr-accordion-caret"><clr-icon shape="angle" dir="right" size="10"></clr-icon></span>
      <span>{title}</span>
    </button>
    {open&&<div className="clr-accordion-content">{children}</div>}
  </div>;
}
