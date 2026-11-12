import { invoke } from "@tauri-apps/api/core";

export interface McpServer { name: string; config: Record<string, unknown>; check_timeout_sec?: number }
export interface McpMarketSource { id: string; name: string; url: string; kind: 'registry' | 'json' }
export const mcpMarketSources = () => invoke<McpMarketSource[]>('mcp_market_sources');
export const mcpSaveMarketSources = (sources: McpMarketSource[]) => invoke<McpMarketSource[]>('mcp_save_market_sources',{sources});
export interface McpBinding { agent_id: string; path: string; format: string }
export interface McpSource {
  id: string; agent_id: string; agent_name: string; path: string; format: string;
  kind: string; servers: McpServer[]; error: string;
}
export interface McpTarget {
  agent_id: string; name: string; installed: boolean; writable: boolean;
  binding: McpBinding | null; managed: string[]; can_restore: boolean;
  /** 尚无已验证文件适配的工具，仅支持在原工具中配置。 */
  app_managed: boolean;
  guide: McpGuide | null; blocked_reason: string;
  reload_notice: string;
}
export interface McpGuide { agent_id: string; path: string; format: string; docs_url: string; notes: string; checked_at: string; status: string }
export interface McpInventory { servers: McpServer[]; sources: McpSource[]; targets: McpTarget[]; warnings: string[]; guides: McpGuide[] }
export interface McpPreview { ticket: string; path: string; actions: { name: string; kind: string }[] }
export const mcpInventory = () => invoke<McpInventory>("mcp_inventory");
export const mcpSaveServer = (server: McpServer) => invoke<void>("mcp_save_server", { server });
export const mcpRemoveServer = (name: string) => invoke<void>("mcp_remove_server", { name });
export const mcpSaveBinding = (binding: McpBinding) => invoke<void>("mcp_save_binding", { binding });
export const mcpAdoptSource = (path: string, format: string, agentId: string) =>
  invoke<string>("mcp_adopt_source", { path, format, agentId });
export const mcpPreview = (agentId: string, names: string[]) => invoke<McpPreview>("mcp_preview", { agentId, names });
export const mcpApply = (ticket: string) => invoke<string>("mcp_apply", { ticket });
export const mcpRestore = (agentId: string) => invoke<string>("mcp_restore", { agentId });
export const mcpPreviewChange = (source: McpSource, server: McpServer, operation: string, config: Record<string,unknown> | null = null) => invoke<McpPreview>('mcp_preview_change', {request:{agentId:source.agent_id,path:source.path,format:source.format,name:server.name,operation,config,expected:server.config,kind:source.kind}});
export interface McpProbeResult {state:string;code:string;phase:string;message:string;details:string;server_name:string;server_version:string;protocol:string;tool_count:number;tools:{name:string;description:string}[];elapsed_ms:number;checked_at:number;partial:boolean}
export const mcpProbeCheck=(id:string,server:McpServer)=>invoke<McpProbeResult>('mcp_probe_check',{request:{id,server}});
export const mcpProbeCancel=(id:string)=>invoke<void>('mcp_probe_cancel',{id});

export const mcpFormats=[{id:'json',name:'JSON · mcpServers'},{id:'codex',name:'Codex · TOML'},{id:'grok',name:'Grok · TOML'},{id:'gemini',name:'Gemini / Qwen / iFlow'},{id:'opencode',name:'OpenCode / Kilo'},{id:'posit',name:'Posit Assistant'},{id:'zcode',name:'Zcode · mcp.servers'},{id:'vscode',name:'VS Code · servers'},{id:'cline',name:'Cline CLI · transport 对象'},{id:'kimi',name:'Kimi Code'},{id:'antigravity',name:'Antigravity · serverUrl'},{id:'copilot',name:'Copilot CLI'},{id:'desktop',name:'Claude Desktop · STDIO'},{id:'crush',name:'Crush · mcp'},{id:'pochi',name:'Pochi · mcp'}];
