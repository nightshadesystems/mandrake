import React from 'react';
export function Label({status,accent,clickable,dismissable,onDismiss,badge,className='',children,...rest}){
  const cls=['label',status?'label-'+status:'',accent?'label-accent':'',clickable?'clickable':'',className].filter(Boolean).join(' ');
  return <span className={cls} {...rest}>{children}{badge!=null&&<Badge>{badge}</Badge>}{dismissable&&<button className="label-dismiss" aria-label="Dismiss" onClick={onDismiss}>×</button>}</span>;
}
export function Badge({status,accent,className='',children,...rest}){
  const cls=['badge',status?'badge-'+status:'',accent?'badge-accent':'',className].filter(Boolean).join(' ');
  return <span className={cls} {...rest}>{children}</span>;
}
