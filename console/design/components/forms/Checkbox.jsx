import React from 'react';
export function Checkbox({label,indeterminate,className='',...rest}){
  const ref=React.useRef();
  React.useEffect(()=>{if(ref.current)ref.current.indeterminate=!!indeterminate;},[indeterminate]);
  const id=React.useId();
  return <div className={'clr-checkbox-wrapper '+className}>
    <input ref={ref} type="checkbox" id={id} className={indeterminate?'indeterminate':''} {...rest}/>
    {label&&<label htmlFor={id}>{label}</label>}
  </div>;
}
export function Radio({label,className='',...rest}){
  const id=React.useId();
  return <div className={'clr-radio-wrapper '+className}><input type="radio" id={id} {...rest}/>{label&&<label htmlFor={id}>{label}</label>}</div>;
}
export function Toggle({label,labelLeft,className='',...rest}){
  const id=React.useId();
  return <div className={['clr-toggle-wrapper',labelLeft?'clr-toggle-right':'',className].filter(Boolean).join(' ')}>
    <input type="checkbox" id={id} {...rest}/>{label&&<label htmlFor={id}>{label}</label>}
  </div>;
}
