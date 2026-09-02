import React from 'react';
export function FormField({label,helper,error,success,required,htmlFor,children,className=''}){
  const state=error?'clr-error':success?'clr-success':'';
  const sub=error||success||helper;
  return <div className={['clr-form-control',state,className].filter(Boolean).join(' ')}>
    {label&&<label className="clr-control-label" htmlFor={htmlFor}>{label}{required&&<span className="clr-required">*</span>}</label>}
    {children}
    {sub&&<span className="clr-subtext">{(error||success)&&<clr-icon shape={error?'exclamation-circle':'check-circle'} size="12" class="is-solid"></clr-icon>}{sub}</span>}
  </div>;
}
