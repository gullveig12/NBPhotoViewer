import {build} from 'vite';
import ts from 'typescript';
// In-process TypeScript transpilation also works in constrained Windows environments.
const typescriptPlugin=()=>({
 name:'typescript-in-process',enforce:'pre',
 transform(code,id){if(!/\.[jt]sx?$/.test(id))return null;code=code.replaceAll('process.env.NODE_ENV','"production"');if(id.includes('node_modules'))return {code,map:null};return {code:ts.transpileModule(code,{compilerOptions:{target:ts.ScriptTarget.ES2022,module:ts.ModuleKind.ESNext,jsx:ts.JsxEmit.ReactJSX,sourceMap:true},fileName:id}).outputText,map:null}}
});
await build({configFile:false,esbuild:false,resolve:{preserveSymlinks:true},plugins:[typescriptPlugin()],worker:{plugins:()=>[typescriptPlugin()]},build:{target:'esnext',minify:false}});
