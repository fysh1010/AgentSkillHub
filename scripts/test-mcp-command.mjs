import assert from 'node:assert/strict';
import {splitMcpCommand,joinMcpCommand} from '../src/utils/mcpCommand.ts';
for(const [command,args] of [
 ['cmd',['/c','npx','-y','yuque-mcp']],
 ['D:\\Program Files\\nodejs\\node.exe',['D:\\my project\\server.js','--value','value with spaces']],
 ['node',['','a"b','D:\\path with spaces\\','中文参数']],
 ["D:/O'Reilly/node.exe",["it's-a-test",'a\\"b',"'quoted'"]],
])assert.deepEqual(splitMcpCommand(joinMcpCommand(command,args)),[command,...args]);
assert.deepEqual(splitMcpCommand('cmd /c npx -y yuque-mcp'),['cmd','/c','npx','-y','yuque-mcp']);
assert.deepEqual(splitMcpCommand('node "D:/my tools/server.mjs" --label \'hello world\''),['node','D:/my tools/server.mjs','--label','hello world']);
assert.throws(()=>splitMcpCommand('node "bad'));
console.log('MCP command: quoting, paths, empty arguments and incomplete quotes passed');
