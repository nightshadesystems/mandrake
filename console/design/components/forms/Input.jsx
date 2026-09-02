import React from 'react';
export function Input({className='',...rest}){return <input className={'clr-input '+className} {...rest}/>;}
export function Textarea({className='',...rest}){return <textarea className={'clr-textarea '+className} {...rest}></textarea>;}
export function NumberInput({className='',style,...rest}){return <input type="number" className={'clr-input '+className} style={{maxWidth:120,...style}} {...rest}/>;}
export function InputGroup({prefix,suffix,children,className=''}){
  return <div className={'clr-input-group '+className}>
    {prefix&&<span className="clr-input-group-addon">{prefix}</span>}
    {children}
    {suffix&&<span className="clr-input-group-addon">{suffix}</span>}
  </div>;
}
