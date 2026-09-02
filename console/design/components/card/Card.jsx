import React from 'react';
export function Card({header,footer,clickable,img,mediaTitle,mediaSub,className='',children,onClick,...rest}){
  const cls=['card',clickable?'clickable':'',className].filter(Boolean).join(' ');
  return <div className={cls} onClick={onClick} {...rest}>
    {header&&<div className="card-header">{header}</div>}
    {img&&<img className="card-img" src={img} alt="" style={{width:'100%',display:'block'}}/>}
    {children}
    {footer&&<div className="card-footer">{footer}</div>}
  </div>;
}
export function CardBlock({title,text,className='',children}){
  return <div className={'card-block '+className}>{title&&<div className="card-title">{title}</div>}{text&&<div className="card-text">{text}</div>}{children}</div>;
}
export function CardMediaBlock({img,title,sub}){
  return <div className="card-block"><div className="card-media-block">{img&&<img className="card-media-image" src={img} alt=""/>}<div><div className="card-title" style={{fontSize:14,margin:0}}>{title}</div><div className="card-text">{sub}</div></div></div></div>;
}
