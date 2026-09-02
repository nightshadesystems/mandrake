import React from 'react';
export function Modal({open,title,size,onClose,footer,children,className=''}){
  if(!open)return null;
  const sz=size?'modal-'+size:'';
  return <React.Fragment>
    <div className="modal-backdrop" onClick={onClose}></div>
    <div className={['modal',sz,className].filter(Boolean).join(' ')}>
      <div className="modal-dialog"><div className="modal-content" role="dialog" aria-modal="true">
        <div className="modal-header"><span className="modal-title">{title}</span><button className="close" aria-label="Close" onClick={onClose}>×</button></div>
        <div className="modal-body">{children}</div>
        {footer&&<div className="modal-footer">{footer}</div>}
      </div></div>
    </div>
  </React.Fragment>;
}
export function SidePanel({open,title,onClose,footer,children,width}){
  if(!open)return null;
  return <React.Fragment>
    <div className="modal-backdrop" onClick={onClose}></div>
    <div className="side-panel" style={width?{width}:undefined} role="dialog" aria-modal="true">
      <div className="modal-header"><span className="modal-title">{title}</span><button className="close" aria-label="Close" onClick={onClose}>×</button></div>
      <div className="modal-body">{children}</div>
      {footer&&<div className="modal-footer">{footer}</div>}
    </div>
  </React.Fragment>;
}
