import React from 'react';
const shapes={info:'info-circle',success:'check-circle',warning:'exclamation-triangle',danger:'exclamation-circle'};
export function Alert({status='info',appLevel,sm,closable,onClose,actions,items,children,className=''}){
  const [closed,setClosed]=React.useState(false);
  if(closed)return null;
  const cls=['alert','alert-'+status,appLevel?'alert-app-level':'',sm?'alert-sm':'',className].filter(Boolean).join(' ');
  const close=()=>{setClosed(true);onClose&&onClose();};
  const body=items?<div className="alert-items">{items.map((it,i)=><div key={i} className="alert-item"><span className="alert-text">{it}</span></div>)}</div>:<span className="alert-text">{children}</span>;
  return <div className={cls} role="alert">
    <clr-icon class="alert-icon is-solid" shape={shapes[status]} size={sm?14:16}></clr-icon>
    {body}
    {actions&&<div className="alert-actions">{actions.map((a,i)=><button key={i} className="alert-action" onClick={a.onClick}>{a.label}</button>)}</div>}
    {closable&&<button className="close" aria-label="Close" onClick={close}>×</button>}
  </div>;
}
