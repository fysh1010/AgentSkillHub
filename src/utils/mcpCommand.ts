/** 命令输入框负责拆分参数；不隐式调用 shell，不展开管道或重定向。 */
export function splitMcpCommand(text:string):string[]{
 const result:string[]=[];let word='';let quote='';let started=false;
 for(let i=0;i<text.length;i++){
  const c=text[i];
  if(!quote&&/\s/.test(c)){if(started){result.push(word);word='';started=false;}continue;}
  if(c==='\\'&&quote!=="'"){
   let end=i;while(text[end]==='\\')end++;const n=end-i;
   if(text[end]==='"'){word+='\\'.repeat(Math.floor(n/2));if(n%2)word+='"';else quote=quote==='"'?'':'"';started=true;i=end;continue;}
   word+='\\'.repeat(n);i=end-1;started=true;continue;
  }
  if((c==='"'||c==="'")&&(!quote||quote===c)){quote=quote?'':c;started=true;continue;}
  word+=c;started=true;
 }
 if(quote)throw new Error('命令中的引号未闭合');if(started)result.push(word);return result;
}
export function joinMcpCommand(command:string,args:unknown=[]):string {
 const quote=(value:string)=>!value||/[\s"']/.test(value)?'"'+value.replace(/(\\*)"/g,'$1$1\\"').replace(/(\\+)$/,'$1$1')+'"':value;
 return [command,...(Array.isArray(args)?args.map(String):[])].map(quote).join(' ');
}
