/**
 * @license
 * SPDX-License-Identifier: Apache-2.0
 */

import React, { useState, useEffect, useRef, Component } from "react";
import { motion, AnimatePresence } from "motion/react";
import { 
  Search, 
  Compass, 
  Anchor, 
  Cpu, 
  Shield, 
  Activity, 
  Terminal, 
  Layers, 
  ChevronRight,
  Globe,
  Lock,
  Zap,
  Layout,
  Settings,
  CpuIcon,
  Server,
  Database,
  EyeOff
} from "lucide-react";
import { Input } from "@/components/ui/input";
import { Button } from "@/components/ui/button";
import { ScrollArea } from "@/components/ui/scroll-area";
import { Badge } from "@/components/ui/badge";
import { Separator } from "@/components/ui/separator";
import { Tooltip, TooltipContent, TooltipProvider, TooltipTrigger } from "@/components/ui/tooltip";
import { 
  Dialog, 
  DialogContent, 
  DialogHeader, 
  DialogTitle, 
  DialogDescription,
  DialogFooter
} from "@/components/ui/dialog";
import {
  Select,
  SelectContent,
  SelectItem,
  SelectTrigger,
  SelectValue,
} from "@/components/ui/select";
import { Switch } from "@/components/ui/switch";
import { Label } from "@/components/ui/label";
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs";
import { Slider } from "@/components/ui/slider";
import { interpretIntent, AgentPlan } from "./services/geminiService";
import { mcpManager, MCPServerConfig } from "./services/mcpService";
import { webLLMService, ModelStatus } from "./services/webLLMService";
import { digitalWake } from "./services/vectorService";
import { citadelVault, VaultKey } from "./services/vaultService";



export default function App() {
  return (
    <TooltipProvider>
      <AppContent />
    </TooltipProvider>
  );
}

function AppContent() {
  const [intent, setIntent] = useState("");
  const [isProcessing, setIsProcessing] = useState(false);
  const [plan, setPlan] = useState<AgentPlan | null>(null);
  const [currentStep, setCurrentStep] = useState(-1);
  const [logs, setLogs] = useState<string[]>(["Sextant OS v1.0.4 initialized.", "Servo engine ready.", "Citadel identity plane active."]);
  const [activeTab, setActiveTab] = useState("https://sextant.io");
  
  // Control Panel State
  const [isSettingsOpen, setIsSettingsOpen] = useState(false);
  const [settings, setSettings] = useState({
    modelEngine: "local-webgpu",
    localModel: "llama-3-8b",
    hardwareAccel: true,
    privacyMode: "citadel",
    mcpEndpoint: "http://localhost:3001",
    temperature: 0.7,
  });

  const [mcpServers, setMcpServers] = useState<MCPServerConfig[]>(mcpManager.getServers());
  const [modelStatus, setModelStatus] = useState<ModelStatus>({
    progress: 100,
    status: "Model Ready",
    vramUsed: 4.2,
    tokensPerSec: 24.5
  });
  const [isDownloading, setIsDownloading] = useState(false);

  // Vault State
  const [vaultLocked, setVaultLocked] = useState(citadelVault.getIsLocked());
  const [vaultKeys, setVaultKeys] = useState<VaultKey[]>([]);
  const [passphrase, setPassphrase] = useState("");

  // Physical Consent State (Captain's Key)
  const [isConsentPromptOpen, setIsConsentPromptOpen] = useState(false);
  const [consentResolver, setConsentResolver] = useState<{ resolve: (v: boolean) => void } | null>(null);

  useEffect(() => {
    const handleConsentRequest = (e: any) => {
      setIsConsentPromptOpen(true);
      setConsentResolver({ resolve: e.detail.resolve });
    };

    window.addEventListener("sextant:physical-consent-request", handleConsentRequest);
    return () => window.removeEventListener("sextant:physical-consent-request", handleConsentRequest);
  }, []);

  const addLog = (msg: string) => {
    setLogs(prev => [...prev.slice(-20), `[${new Date().toLocaleTimeString()}] ${msg}`]);
  };

  const handleIntent = async (e: React.FormEvent) => {
    e.preventDefault();
    if (!intent.trim()) return;

    setIsProcessing(true);
    setPlan(null);
    setCurrentStep(-1);
    addLog(`Interpreting intent: "${intent}"`);

    try {
      const result = await interpretIntent(intent);
      setPlan(result);
      addLog(`Plan generated: ${result.summary}`);
      
      // Simulate execution
      for (let i = 0; i < result.steps.length; i++) {
        setCurrentStep(i);
        const step = result.steps[i];
        addLog(`Executing: ${step.action} - ${step.description}`);
        
        // Simulate "Captain's Key" workflow for MCP tool calls
        if (step.action.toLowerCase().includes("mcp") || step.action.toLowerCase().includes("tool")) {
          addLog("Vault intercept: Requesting cryptographic signature for MCP payload...");
          try {
            const signature = await citadelVault.sign(vaultKeys[0]?.id || "default", JSON.stringify(step));
            addLog(`Payload signed via Ed25519. Sig: ${signature.substring(0, 16)}...`);
          } catch (err) {
            addLog("Error: Physical consent denied or vault locked.");
            setIsProcessing(false);
            return;
          }
        }

        await new Promise(r => setTimeout(r, 1500));
      }
      
      addLog("Task completed successfully.");
      
      // Add to Digital Wake
      digitalWake.addEntry(`Completed task: ${result.summary}`);
      addLog("Action recorded in Digital Wake (Local RAG).");

      setActiveTab(result.steps[result.steps.length - 1]?.target || activeTab);
    } catch (error) {
      addLog("Error: Failed to process intent.");
    } finally {
      setIsProcessing(false);
    }
  };

  return (
    <div className="flex h-screen w-screen bg-citadel-bg text-white font-sans selection:bg-citadel-accent/30 overflow-hidden">
        
        {/* Left Sidebar: Context Vault */}
        <aside className="w-[280px] border-r border-citadel-border bg-citadel-card flex flex-col z-20">
          <div className="p-6 flex items-center gap-3 border-b border-citadel-border">
            <div className="relative h-2 w-2 bg-citadel-accent rounded-full shadow-[0_0_8px_var(--color-citadel-accent)]" />
            <h1 className="micro-label !text-white/70">Context Vault v1.0.4</h1>
          </div>

          <ScrollArea className="flex-1 p-6">
            <div className="space-y-10">
              {/* Agent Status */}
              <div>
                <div className="flex items-center justify-between mb-3">
                  <h3 className="micro-label">Identity Plane</h3>
                </div>
                <div className="space-y-2">
                  <StatusItem 
                    icon={<Shield className="h-3.5 w-3.5" />} 
                    label="Citadel Persona" 
                    value={vaultLocked ? "Locked" : "Active"} 
                    active={!vaultLocked}
                    onClick={() => setIsSettingsOpen(true)}
                  />
                  <StatusItem 
                    icon={<Lock className="h-3.5 w-3.5" />} 
                    label="Hardware Anchor" 
                    value={citadelVault.getHardwareAnchorStatus()} 
                    active={!vaultLocked}
                  />
                  {!vaultLocked && (
                    <div className="text-[10px] mt-2 text-citadel-accent/70 font-mono">
                      ephemeral_alpha_9921_ox
                    </div>
                  )}
                </div>
              </div>

              {/* Local Inference */}
              <div>
                <div className="flex items-center justify-between mb-3">
                  <h3 className="micro-label">Local Inference</h3>
                </div>
                <div className="status-value flex justify-between items-center p-2 px-3 bg-white/5 rounded border border-citadel-border">
                  <span className="text-[13px] text-white/90">Candle Llama-3B</span>
                  <span className="text-[13px] text-citadel-accent">{modelStatus.tokensPerSec} t/s</span>
                </div>
              </div>

              {/* Hardware Layer */}
              <div>
                <div className="flex items-center justify-between mb-3">
                  <h3 className="micro-label">Hardware Layer</h3>
                </div>
                <div className="status-value flex justify-between items-center p-2 px-3 bg-white/5 rounded border border-citadel-border">
                  <span className="text-[13px] text-white/90">wgpu Context</span>
                  <span className="text-[13px] text-[#4ade80]">Nominal</span>
                </div>
                <div className="text-[10px] mt-2 text-citadel-text-dim font-mono">
                  EPYC MILAN | RTX 4090 D
                </div>
              </div>

              {/* Sonar Widget */}
              <div className="sonar-widget mt-auto h-[180px] border border-citadel-border rounded-lg relative flex items-center justify-center overflow-hidden">
                <div className="absolute border border-citadel-accent/10 rounded-full w-[140px] h-[140px]" />
                <div className="absolute border border-citadel-accent/10 rounded-full w-[100px] h-[100px]" />
                <div className="absolute border border-citadel-accent/10 rounded-full w-[60px] h-[60px]" />
                <div className="absolute w-[1px] h-[80%] bg-gradient-to-b from-transparent via-citadel-accent to-transparent rotate-[45deg]" />
                <div className="micro-label absolute bottom-2 !text-citadel-accent">Sonar Scanning...</div>
              </div>

              <Separator className="bg-white/5" />

              {/* Terminal Logs */}
              <div>
                <h3 className="micro-label mb-3">System Logs</h3>
                <div className="bg-black/40 rounded-md p-3 font-mono text-[10px] space-y-1 border border-white/5 h-48 overflow-hidden">
                  {logs.map((log, i) => (
                    <div key={i} className="text-citadel-text-dim leading-relaxed">
                      <span className="text-citadel-accent mr-1">$</span> {log}
                    </div>
                  ))}
                </div>
              </div>

              {/* Active Plan */}
              {plan && (
                <motion.div 
                  initial={{ opacity: 0, y: 10 }}
                  animate={{ opacity: 1, y: 0 }}
                  className="space-y-3"
                >
                  <h3 className="text-[11px] font-mono uppercase tracking-wider text-slate-500">Active Plan</h3>
                  <div className="space-y-2">
                    {plan.steps.map((step, i) => (
                      <div 
                        key={i} 
                        className={`p-2 rounded border text-xs transition-colors ${
                          i === currentStep 
                            ? "bg-nautical-accent/10 border-nautical-accent text-white" 
                            : i < currentStep 
                              ? "bg-white/5 border-white/10 text-slate-500" 
                              : "bg-transparent border-white/5 text-slate-400"
                        }`}
                      >
                        <div className="flex items-center gap-2">
                          <div className={`h-1.5 w-1.5 rounded-full ${i === currentStep ? "bg-nautical-accent animate-pulse" : i < currentStep ? "bg-slate-600" : "bg-white/20"}`} />
                          <span className="font-medium uppercase text-[10px]">{step.action}</span>
                        </div>
                        <p className="mt-1 opacity-80 leading-snug">{step.description}</p>
                      </div>
                    ))}
                  </div>
                </motion.div>
              )}
            </div>
          </ScrollArea>

          <div className="p-4 border-t border-white/5 bg-black/20">
            <div className="flex items-center gap-2 text-[10px] font-mono text-slate-500 uppercase tracking-tighter">
              <div className="h-1.5 w-1.5 rounded-full bg-green-500" />
              <span>Nautical Stack v1.0.4-beta</span>
            </div>
          </div>
        </aside>

        {/* Main Content Area */}
        <main className="flex-1 flex flex-col relative overflow-hidden">
          
          {/* Top Bar: Intent Bar */}
          <header className="h-20 border-b border-citadel-border bg-citadel-bg flex items-center px-10 gap-4 z-10">
            <div className="flex items-center gap-2 mr-4">
              <div className="flex gap-1">
                <div className="h-1.5 w-1.5 rounded-full bg-citadel-accent/20" />
                <div className="h-1.5 w-1.5 rounded-full bg-citadel-accent/20" />
                <div className="h-1.5 w-1.5 rounded-full bg-citadel-accent/20" />
              </div>
            </div>

            <form onSubmit={handleIntent} className="flex-1 relative group">
              <div className="absolute left-4 top-1/2 -translate-y-1/2 micro-label group-focus-within:!text-citadel-accent transition-colors">
                INTENT:
              </div>
              <Input 
                value={intent}
                onChange={(e) => setIntent(e.target.value)}
                placeholder="What is your intent, Commander?"
                className="w-full bg-transparent border-b border-citadel-border rounded-none pl-20 h-11 focus-visible:ring-0 focus-visible:border-citadel-accent transition-all font-mono text-citadel-accent placeholder:text-white/10"
                disabled={isProcessing}
              />
            </form>

            <div className="flex items-center gap-3 ml-4">
              <NavIcon icon={<Globe className="h-4 w-4" />} tooltip="Network" />
              <NavIcon icon={<Lock className="h-4 w-4" />} tooltip="Citadel Privacy" />
              <NavIcon icon={<Zap className="h-4 w-4" />} tooltip="Hardware Accel" />
              <Separator orientation="vertical" className="h-6 bg-white/10" />
              <Button 
                variant="ghost" 
                size="icon" 
                className="rounded-full hover:bg-white/5"
                onClick={() => setIsSettingsOpen(true)}
              >
                <Settings className="h-4 w-4" />
              </Button>
            </div>
          </header>

          {/* Viewport Area */}
          <div className="flex-1 relative bg-citadel-bg p-10">
            <div className="absolute inset-0 opacity-5 pointer-events-none overflow-hidden">
              <div className="absolute inset-0" style={{ backgroundImage: 'radial-gradient(circle at 1px 1px, rgba(255,255,255,0.05) 1px, transparent 0)', backgroundSize: '48px 48px' }} />
            </div>

            <div className="h-full w-full rounded border border-citadel-border overflow-hidden relative bg-black/40 flex flex-col">
              {/* Browser Chrome */}
              <div className="h-10 bg-citadel-card border-b border-citadel-border flex items-center px-4 justify-between">
                <div className="flex items-center gap-4 text-[11px] font-mono text-citadel-text-dim">
                  <div className="flex items-center gap-1.5 uppercase">
                    <span>Servo Render Engine (v0.0.1)</span>
                  </div>
                </div>
                <div className="flex items-center gap-2 micro-label">
                  SSL: Encrypted | WASM Sandbox: Enabled
                </div>
              </div>

              {/* Browser Content (Simulated) */}
              <div className="flex-1 relative overflow-hidden flex items-center justify-center">
                <AnimatePresence mode="wait">
                  {isProcessing ? (
                    <motion.div 
                      key="processing"
                      initial={{ opacity: 0 }}
                      animate={{ opacity: 1 }}
                      exit={{ opacity: 0 }}
                      className="flex flex-col items-center gap-6"
                    >
                      <div className="relative h-32 w-32 flex items-center justify-center">
                        <motion.div 
                          animate={{ rotate: 360 }}
                          transition={{ duration: 8, repeat: Infinity, ease: "linear" }}
                          className="absolute inset-0 border border-dashed border-citadel-accent/20 rounded-full"
                        />
                        <motion.div 
                          animate={{ rotate: -360 }}
                          transition={{ duration: 12, repeat: Infinity, ease: "linear" }}
                          className="absolute inset-2 border border-dashed border-citadel-accent/10 rounded-full"
                        />
                        <div className="absolute inset-0 animate-sonar" />
                        <Compass className="h-12 w-12 text-citadel-accent animate-pulse" />
                      </div>
                      <div className="text-center space-y-2">
                        <h2 className="text-xl font-light tracking-widest text-white uppercase">Scanning Web Surface</h2>
                        <p className="micro-label">Extracting Semantic DOM via Servo Instrumentation</p>
                      </div>
                    </motion.div>
                  ) : (
                    <motion.div 
                      key="content"
                      initial={{ opacity: 0, scale: 0.98 }}
                      animate={{ opacity: 1, scale: 1 }}
                      className="h-full w-full p-8 flex flex-col"
                    >
                      {activeTab === "https://sextant.io" ? (
                        <div className="max-w-2xl mx-auto mt-20 space-y-8">
                          <div className="semantic-dom-node border-l-2 border-citadel-accent/20 pl-[15px] mb-5">
                            <div className="node-tag text-citadel-accent font-mono text-[10px] mb-1 uppercase tracking-widest">h1#title</div>
                            <h1 className="text-4xl font-light tracking-tighter text-white uppercase">Sextant <span className="text-citadel-accent">Browser</span></h1>
                          </div>
                          
                          <div className="semantic-dom-node border-l-2 border-citadel-accent/20 pl-[15px] mb-5">
                            <div className="node-tag text-citadel-accent font-mono text-[10px] mb-1 uppercase tracking-widest">article#summary-agent</div>
                            <p className="text-sm text-white/60 font-light leading-relaxed">
                              Agent analysis complete. Local model indicates a 14% performance increase in memory safety handling on the EPYC Milan architecture when leveraging the Sextant-specific instrumentation layer.
                            </p>
                          </div>

                          <div className="mt-10 border-t border-dashed border-citadel-border pt-5">
                            <div className="micro-label mb-2">Detected Intent_Links:</div>
                            <div className="flex gap-2">
                              <Badge className="bg-white/5 border border-citadel-border px-3 py-1 text-citadel-accent hover:bg-citadel-accent/10 cursor-pointer rounded-none uppercase text-[9px] tracking-widest">Fetch Source Code</Badge>
                              <Badge className="bg-white/5 border border-citadel-border px-3 py-1 text-citadel-accent hover:bg-citadel-accent/10 cursor-pointer rounded-none uppercase text-[9px] tracking-widest">Verify Signature</Badge>
                              <Badge className="bg-white/5 border border-citadel-border px-3 py-1 text-citadel-accent hover:bg-citadel-accent/10 cursor-pointer rounded-none uppercase text-[9px] tracking-widest">Export to Citadel</Badge>
                            </div>
                          </div>
                        </div>
                      ) : (
                        <div className="h-full w-full flex flex-col items-center justify-center text-center space-y-4">
                          <Globe className="h-16 w-16 text-white/10" />
                          <div className="space-y-1">
                            <h3 className="text-2xl font-light text-white">Navigated to {activeTab}</h3>
                            <p className="micro-label">Agent has successfully reached the target destination.</p>
                          </div>
                          <Button 
                            variant="outline" 
                            className="border-citadel-border hover:bg-white/5 rounded-none uppercase text-[10px] tracking-[0.2em]"
                            onClick={() => setActiveTab("https://sextant.io")}
                          >
                            Return to Home
                          </Button>
                        </div>
                      )}
                    </motion.div>
                  )}
                </AnimatePresence>
              </div>
            </div>
            <div className="absolute bottom-5 right-5 font-mono text-[10px] text-slate-500 tracking-widest uppercase">
              Coord: 43.3214° N, 5.3340° E | Marseille_Vault
            </div>
          </div>
        </main>

        {/* Control Panel Modal */}
        <Dialog open={isSettingsOpen} onOpenChange={setIsSettingsOpen}>
          <DialogContent className="sm:max-w-[600px] bg-citadel-card border-citadel-border text-white rounded-none">
            <DialogHeader>
              <DialogTitle className="text-xl font-light tracking-[0.2em] text-white flex items-center gap-2 uppercase">
                <Settings className="h-5 w-5 text-citadel-accent" />
                Sextant Control
              </DialogTitle>
              <DialogDescription className="micro-label">
                Hardware-aware agent configuration
              </DialogDescription>
            </DialogHeader>

            <Tabs defaultValue="brain" className="w-full">
              <TabsList className="grid w-full grid-cols-4 bg-black/40 border border-citadel-border rounded-none h-12">
                <TabsTrigger value="brain" className="micro-label data-[state=active]:text-citadel-accent data-[state=active]:bg-white/5 rounded-none">Pilot</TabsTrigger>
                <TabsTrigger value="mcp" className="micro-label data-[state=active]:text-citadel-accent data-[state=active]:bg-white/5 rounded-none">Tools</TabsTrigger>
                <TabsTrigger value="memory" className="micro-label data-[state=active]:text-citadel-accent data-[state=active]:bg-white/5 rounded-none">Wake</TabsTrigger>
                <TabsTrigger value="privacy" className="micro-label data-[state=active]:text-citadel-accent data-[state=active]:bg-white/5 rounded-none">Vault</TabsTrigger>
              </TabsList>

              <TabsContent value="brain" className="space-y-6 pt-4">
                <div className="space-y-4">
                  <div className="flex flex-col gap-2">
                    <Label className="micro-label">Intelligence Standard</Label>
                    <div className="p-3 rounded bg-citadel-accent/5 border border-citadel-accent/20 flex items-center gap-3">
                      <Zap className="h-5 w-5 text-citadel-accent" />
                      <div>
                        <p className="text-xs font-bold text-white uppercase tracking-widest">WebLLM Powered by WebGPU</p>
                        <p className="text-[10px] text-citadel-text-dim">W3C Standard for local-first private inference.</p>
                      </div>
                    </div>
                  </div>

                  <div className="flex flex-col gap-2">
                    <Label className="micro-label">Model Selection</Label>
                    <div className="flex gap-2">
                      <Select 
                        value={settings.localModel} 
                        onValueChange={(v) => setSettings({...settings, localModel: v})}
                      >
                        <SelectTrigger className="bg-black/30 border-citadel-border flex-1 rounded-none">
                          <SelectValue placeholder="Select Model" />
                        </SelectTrigger>
                        <SelectContent className="bg-citadel-card border-citadel-border text-white">
                          <SelectItem value="llama-3-8b">Llama 3 (8B Parameters)</SelectItem>
                          <SelectItem value="mistral-7b">Mistral (7B Parameters)</SelectItem>
                          <SelectItem value="gemma-2b">Gemma (2B Parameters)</SelectItem>
                          <SelectItem value="phi-3-mini">Phi-3 Mini (3.8B)</SelectItem>
                        </SelectContent>
                      </Select>
                      <Button 
                        variant="outline" 
                        className="border-citadel-accent/30 text-citadel-accent hover:bg-citadel-accent/10 rounded-none uppercase text-[10px] tracking-widest"
                        onClick={() => {
                          setIsDownloading(true);
                          webLLMService.initialize((status) => {
                            setModelStatus(status);
                            if (status.progress === 100) {
                              setIsDownloading(false);
                              addLog("Local model weights verified in cache.");
                            }
                          });
                        }}
                        disabled={isDownloading}
                      >
                        {isDownloading ? `${modelStatus.progress}%` : "Pull Weights"}
                      </Button>
                    </div>
                  </div>

                  <div className="flex items-center justify-between p-3 rounded bg-black/20 border border-citadel-border">
                    <div className="space-y-0.5">
                      <Label className="text-sm font-light text-white">Hardware Acceleration (wgpu)</Label>
                      <p className="text-[10px] text-citadel-text-dim">Direct GPU access shared with Servo engine</p>
                    </div>
                    <Switch 
                      checked={settings.hardwareAccel} 
                      onCheckedChange={(v) => setSettings({...settings, hardwareAccel: v})}
                    />
                  </div>

                  <div className="space-y-3">
                    <div className="flex justify-between">
                      <Label className="text-[11px] font-mono uppercase text-slate-500">Temperature</Label>
                      <span className="text-[10px] font-mono text-nautical-accent">{settings.temperature}</span>
                    </div>
                    <Slider 
                      value={[settings.temperature]} 
                      max={1} 
                      step={0.1} 
                      onValueChange={(vals) => setSettings({...settings, temperature: vals[0]})}
                      className="py-2"
                    />
                  </div>
                </div>
              </TabsContent>

              <TabsContent value="mcp" className="space-y-6 pt-4">
                <div className="space-y-4">
                  <div className="flex flex-col gap-2">
                    <Label className="text-[11px] font-mono uppercase text-slate-500">Tooling Standard (MCP)</Label>
                    <p className="text-[10px] text-slate-400 leading-relaxed">
                      Model Context Protocol (MCP) provides a standardized "USB-C" for AI tools. 
                      Sextant connects to these servers via a secure async handshake.
                    </p>
                  </div>

                  <div className="space-y-2">
                    <Label className="text-[11px] font-mono uppercase text-slate-500">Active MCP Servers</Label>
                    <div className="space-y-2">
                      {mcpServers.map(server => (
                        <div key={server.id} className="p-3 rounded-lg bg-black/30 border border-white/10 flex items-center justify-between">
                          <div className="flex items-center gap-3">
                            <div className={`h-2 w-2 rounded-full ${server.status === 'connected' ? 'bg-green-500' : 'bg-slate-600'}`} />
                            <div>
                              <p className="text-xs font-bold text-white">{server.name}</p>
                              <p className="text-[9px] font-mono text-slate-500">{server.endpoint}</p>
                            </div>
                          </div>
                          <div className="flex gap-1">
                            {server.tools.map(tool => (
                              <Badge key={tool} variant="outline" className="text-[8px] border-white/5 bg-white/5 text-slate-400">
                                {tool}
                              </Badge>
                            ))}
                          </div>
                        </div>
                      ))}
                    </div>
                  </div>
                </div>
              </TabsContent>

              <TabsContent value="memory" className="space-y-6 pt-4">
                <div className="space-y-4">
                  <div className="flex flex-col gap-2">
                    <Label className="text-[11px] font-mono uppercase text-slate-500">Memory Standard (Digital Wake)</Label>
                    <div className="p-3 rounded-lg bg-black/30 border border-white/10 flex items-center gap-3">
                      <Database className="h-5 w-5 text-nautical-accent" />
                      <div>
                        <p className="text-xs font-bold text-white uppercase">Local-First Vector RAG</p>
                        <p className="text-[10px] text-slate-400">SQLite-VSS powered knowledge base.</p>
                      </div>
                    </div>
                  </div>

                  <div className="grid grid-cols-2 gap-3">
                    <div className="p-3 rounded-lg bg-black/20 border border-white/5 space-y-1">
                      <span className="text-[9px] font-mono text-slate-500 uppercase">Total Entries</span>
                      <p className="text-lg font-bold text-white">{digitalWake.getStats().totalEntries}</p>
                    </div>
                    <div className="p-3 rounded-lg bg-black/20 border border-white/5 space-y-1">
                      <span className="text-[9px] font-mono text-slate-500 uppercase">Storage</span>
                      <p className="text-lg font-bold text-white">{digitalWake.getStats().storageUsed}</p>
                    </div>
                  </div>

                  <div className="p-3 rounded-lg bg-nautical-accent/5 border border-nautical-accent/20">
                    <p className="text-[10px] text-slate-400 leading-relaxed">
                      Your "Digital Wake" is stored in a portable SQLite file, encrypted using your Citadel keys. 
                      It never leaves your hardware.
                    </p>
                  </div>
                </div>
              </TabsContent>

              <TabsContent value="privacy" className="space-y-6 pt-4">
                <div className="space-y-4">
                  <div className="flex flex-col gap-2">
                    <Label className="text-[11px] font-mono uppercase text-slate-500">Cryptographic Standard</Label>
                    <div className="p-3 rounded-lg bg-nautical-accent/5 border border-nautical-accent/20 flex items-center gap-3">
                      <Lock className="h-5 w-5 text-nautical-accent" />
                      <div>
                        <p className="text-xs font-bold text-white uppercase">Rust-Compliant Security</p>
                        <p className="text-[10px] text-slate-400">Ed25519-Dalek, ECDSA, and DSA support.</p>
                      </div>
                    </div>
                  </div>

                  {vaultLocked ? (
                    <div className="space-y-4 p-4 rounded-lg bg-black/40 border border-white/10">
                      <div className="space-y-2">
                        <Label className="text-[10px] font-mono uppercase text-slate-500">Master Passphrase</Label>
                        <Input 
                          type="password"
                          value={passphrase}
                          onChange={(e) => setPassphrase(e.target.value)}
                          placeholder="Enter passphrase to unlock..."
                          className="bg-black/30 border-white/10"
                        />
                      </div>
                      <Button 
                        className="w-full bg-nautical-accent hover:bg-nautical-accent/80 text-white font-mono uppercase text-xs"
                        onClick={async () => {
                          const success = await citadelVault.unlock(passphrase);
                          if (success) {
                            setVaultLocked(false);
                            setVaultKeys(citadelVault.getKeys());
                            addLog("Citadel Vault unlocked. Cryptographic identities active.");
                          } else {
                            addLog("Error: Invalid passphrase.");
                          }
                        }}
                      >
                        Unlock Citadel
                      </Button>
                      <p className="text-[9px] text-center text-slate-600 font-mono">Hint: Use "commander" for prototype</p>
                    </div>
                  ) : (
                    <div className="space-y-4">
                      <div className="flex items-center justify-between">
                        <Label className="text-[11px] font-mono uppercase text-slate-500">Active Identities</Label>
                        <Button 
                          variant="ghost" 
                          size="sm" 
                          className="h-6 text-[9px] font-mono text-nautical-accent hover:bg-nautical-accent/10"
                          onClick={async () => {
                            await citadelVault.generateKey("Ed25519");
                            setVaultKeys([...citadelVault.getKeys()]);
                            addLog("New Ed25519 identity generated via Dalek-simulated core.");
                          }}
                        >
                          + New Key
                        </Button>
                      </div>
                      <div className="space-y-2">
                        {vaultKeys.map(key => (
                          <div key={key.id} className="p-3 rounded-lg bg-black/30 border border-white/10 space-y-2">
                            <div className="flex items-center justify-between">
                              <Badge variant="outline" className="text-[8px] border-nautical-accent/30 text-nautical-accent">
                                {key.type}
                              </Badge>
                              <span className="text-[9px] font-mono text-slate-500">{key.id}</span>
                            </div>
                            <div className="space-y-1">
                              <div className="text-[8px] font-mono text-slate-500 uppercase">DID</div>
                              <div className="font-mono text-[9px] text-nautical-accent break-all bg-black/20 p-2 rounded border border-white/5">
                                {key.did}
                              </div>
                            </div>
                            <div className="space-y-1">
                              <div className="text-[8px] font-mono text-slate-500 uppercase">Public Key</div>
                              <div className="font-mono text-[9px] text-slate-400 break-all bg-black/20 p-2 rounded border border-white/5">
                                {key.publicKey}
                              </div>
                            </div>
                          </div>
                        ))}
                      </div>
                      <Button 
                        variant="outline" 
                        className="w-full border-red-500/30 text-red-500 hover:bg-red-500/10 font-mono uppercase text-xs"
                        onClick={() => {
                          citadelVault.lock();
                          setVaultLocked(true);
                          setVaultKeys([]);
                          setPassphrase("");
                          addLog("Citadel Vault locked. Memory and identities encrypted.");
                        }}
                      >
                        Lock Vault
                      </Button>
                    </div>
                  )}
                </div>
              </TabsContent>

              <TabsContent value="network" className="space-y-6 pt-4">
                <div className="space-y-4">
                  <div className="flex flex-col gap-2">
                    <Label className="text-[11px] font-mono uppercase text-slate-500">MCP Endpoint</Label>
                    <Input 
                      value={settings.mcpEndpoint} 
                      onChange={(e) => setSettings({...settings, mcpEndpoint: e.target.value})}
                      className="bg-black/30 border-white/10"
                      placeholder="http://localhost:3001"
                    />
                    <p className="text-[9px] text-slate-500 font-mono">Model Context Protocol for tool communication</p>
                  </div>

                  <div className="space-y-2">
                    <Label className="text-[11px] font-mono uppercase text-slate-500">Connected Tools</Label>
                    <div className="space-y-1">
                      <ConnectedTool name="Citadel Identity" status="Connected" />
                      <ConnectedTool name="Gitea Storage" status="Connected" />
                      <ConnectedTool name="Local Filesystem" status="Restricted" />
                    </div>
                  </div>
                </div>
              </TabsContent>
            </Tabs>

            <DialogFooter className="mt-6 border-t border-citadel-border pt-4">
              <Button 
                variant="outline" 
                className="border-citadel-border hover:bg-white/5 rounded-none micro-label"
                onClick={() => setIsSettingsOpen(false)}
              >
                Close
              </Button>
              <Button 
                className="bg-citadel-accent hover:bg-citadel-accent/80 text-black rounded-none micro-label"
                onClick={() => {
                  addLog(`Settings updated: ${settings.localModel.toUpperCase()} engine active.`);
                  setIsSettingsOpen(false);
                }}
              >
                Apply Changes
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>

        {/* The Captain's Key: Physical Consent Prompt */}
        <Dialog open={isConsentPromptOpen} onOpenChange={setIsConsentPromptOpen}>
          <DialogContent className="bg-nautical-900 border-nautical-accent/30 text-white max-w-sm">
            <DialogHeader>
              <DialogTitle className="flex items-center gap-2 text-nautical-accent">
                <Zap className="h-5 w-5" />
                THE CAPTAIN'S KEY
              </DialogTitle>
              <DialogDescription className="text-slate-400 font-mono text-xs">
                A hardware-anchored cryptographic signature has been requested for an MCP tool execution.
              </DialogDescription>
            </DialogHeader>
            
            <div className="py-6 flex flex-col items-center justify-center space-y-4">
              <div className="relative h-20 w-20 flex items-center justify-center">
                <div className="absolute inset-0 bg-nautical-accent/20 rounded-full animate-ping" />
                <div className="absolute inset-0 bg-nautical-accent/10 rounded-full animate-sonar" />
                <Shield className="h-10 w-10 text-nautical-accent relative z-10" />
              </div>
              <p className="text-sm font-bold text-center animate-pulse">TOUCH YOUR SECURITY KEY NOW</p>
              <p className="text-[10px] text-slate-500 font-mono uppercase">Waiting for FIDO2 / YubiKey assertion...</p>
            </div>

            <DialogFooter>
              <Button 
                variant="ghost" 
                className="text-slate-500 hover:text-white text-xs"
                onClick={() => {
                  consentResolver?.resolve(false);
                  setIsConsentPromptOpen(false);
                }}
              >
                Cancel
              </Button>
              <Button 
                className="bg-nautical-accent hover:bg-nautical-accent/80 text-white font-mono uppercase text-xs"
                onClick={() => {
                  consentResolver?.resolve(true);
                  setIsConsentPromptOpen(false);
                }}
              >
                Simulate Touch
              </Button>
            </DialogFooter>
          </DialogContent>
        </Dialog>
      </div>
  );
}

function StatusItem({ icon, label, value, active, onClick }: { icon: React.ReactNode, label: string, value: string, active?: boolean, onClick?: () => void }) {
  return (
    <div 
      className={`flex items-center justify-between p-2 rounded bg-white/5 border border-citadel-border transition-colors ${onClick ? 'cursor-pointer hover:bg-white/10' : ''}`}
      onClick={onClick}
    >
      <div className="flex items-center gap-2">
        <div className={active ? "text-citadel-accent" : "text-citadel-text-dim"}>
          {icon}
        </div>
        <span className="micro-label !text-white/60">{label}</span>
      </div>
      <span className="status-value">{value}</span>
    </div>
  );
}

function ConnectedTool({ name, status }: { name: string, status: string }) {
  return (
    <div className="flex items-center justify-between p-2 rounded bg-black/40 border border-citadel-border">
      <span className="micro-label !text-white/70">{name}</span>
      <Badge variant="outline" className={`text-[8px] h-4 rounded-none ${status === 'Connected' ? 'border-citadel-accent/50 text-citadel-accent' : 'border-citadel-warning/50 text-citadel-warning'}`}>
        {status.toUpperCase()}
      </Badge>
    </div>
  );
}

function NavIcon({ icon, tooltip }: { icon: React.ReactNode, tooltip: string }) {
  return (
    <Tooltip>
      <TooltipTrigger asChild>
        <Button variant="ghost" size="icon" className="h-8 w-8 rounded-full hover:bg-white/5 text-citadel-text-dim hover:text-white transition-colors">
          {icon}
        </Button>
      </TooltipTrigger>
      <TooltipContent side="bottom" className="bg-citadel-card border-citadel-border text-[10px] font-mono uppercase tracking-widest">
        {tooltip}
      </TooltipContent>
    </Tooltip>
  );
}

function FeatureCard({ icon, title, desc }: { icon: React.ReactNode, title: string, desc: string }) {
  return (
    <div className="p-4 rounded-xl border border-white/5 bg-white/5 hover:bg-white/10 transition-colors group cursor-default">
      <div className="h-10 w-10 rounded-lg bg-nautical-accent/10 flex items-center justify-center text-nautical-accent mb-3 group-hover:scale-110 transition-transform">
        {icon}
      </div>
      <h4 className="text-sm font-bold text-white mb-1 uppercase tracking-tight">{title}</h4>
      <p className="text-xs text-slate-500 leading-relaxed">{desc}</p>
    </div>
  );
}

