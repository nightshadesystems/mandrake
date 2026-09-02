import React from 'react';
export function Password({className='',...rest}){
  const [show,setShow]=React.useState(false);
  return <div className="clr-password-wrapper">
    <input type={show?'text':'password'} className={'clr-input '+className} {...rest}/>
    <button type="button" className="clr-password-toggle" aria-label="Show password" onClick={()=>setShow(s=>!s)}><clr-icon shape={show?'eye-hide':'eye'} size="16"></clr-icon></button>
  </div>;
}
export function Range({className='',...rest}){return <input type="range" className={'clr-range '+className} {...rest}/>;}
export function FileInput({buttonText='Browse…',accept,multiple,onChange,className=''}){
  const [names,setNames]=React.useState('');
  const ref=React.useRef();
  return <div className={'clr-file-label '+className}>
    <input ref={ref} type="file" className="clr-file-input" accept={accept} multiple={multiple} onChange={e=>{setNames([...e.target.files].map(f=>f.name).join(', '));onChange&&onChange(e);}}/>
    <button type="button" className="btn btn-sm" onClick={()=>ref.current.click()}><clr-icon shape="upload" size="14"></clr-icon>{buttonText}</button>
    <span className="clr-file-name">{names||'No file selected'}</span>
  </div>;
}
