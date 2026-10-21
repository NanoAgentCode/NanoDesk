import { describe, expect, it, vi } from "vitest";
import type { AgentRun } from "../types";
import { steerConversationTask, taskSteeringIntent } from "./taskSteering";

const main = {id:"main-run",conversation_id:"main",status:"running"} as AgentRun;
const child = {id:"child-run",conversation_id:"temporary",status:"running"} as AgentRun;
function fixture(run = main, links = {parent:null,child:null} as {parent:AgentRun|null;child:AgentRun|null}) {
  const invoke=vi.fn(async(command:string,args:Record<string,unknown>):Promise<unknown>=>{
    if(command==="list_agent_runs")return [run];
    if(command==="get_task_interruption")return args.runId===run.id?links:{parent:run,child:null};
    if(command==="start_temporary_task")return {id:"temporary"};
    if(command==="return_to_parent_task")return "main";
    return undefined;
  });
  const send=(text:string)=>steerConversationTask({conversationId:run.conversation_id,text,accessMode:"ask",allowCommand:false,invoke});
  return {invoke,send};
}
describe("ordinary conversation task steering",()=>{
  it.each(["暂停任务","请先暂停。","先停一下","等一下"])("recognizes a normal pause request: %s",text=>expect(taskSteeringIntent(text)).toBe("pause"));
  it.each(["继续任务","请继续之前的任务。","恢复主任务","回到主线","可以继续了"])("recognizes a normal resume request: %s",text=>expect(taskSteeringIntent(text)).toBe("resume"));
  it("inserts an ordinary message while the main task is streaming and preserves current permissions",async()=>{
    const {invoke,send}=fixture();
    const result=await send("先查一下这个报错，再把主线剩余实现改成新的接口");
    expect(result).toEqual({handled:true,conversationId:"temporary"});
    expect(invoke).toHaveBeenCalledWith("start_temporary_task",{parentRunId:"main-run",content:"先查一下这个报错，再把主线剩余实现改成新的接口",accessMode:"ask",allowCommand:false});
    expect(invoke).not.toHaveBeenCalledWith("create_agent_run",expect.anything());
  });
  it("uses a new message after pausing as a temporary task rather than losing the paused mainline",async()=>{
    const {invoke,send}=fixture({...main,status:"paused"});
    await send("补充要求：保留已完成步骤，先验证配置，再继续修改");
    expect(invoke).toHaveBeenCalledWith("start_temporary_task",expect.objectContaining({parentRunId:"main-run"}));
  });
  it("pauses through ordinary chat and records the user's message",async()=>{
    const {invoke,send}=fixture();
    await send("先暂停任务");
    expect(invoke).toHaveBeenCalledWith("pause_background_agent",{runId:"main-run"});
    expect(invoke).toHaveBeenCalledWith("append_message",expect.objectContaining({draft:expect.objectContaining({content:"先暂停任务"})}));
  });
  it("resumes a checkpoint through ordinary chat without creating a duplicate task",async()=>{
    const {invoke,send}=fixture({...main,status:"paused"});
    await send("继续未完成的任务");
    expect(invoke).toHaveBeenCalledWith("respond_background_agent",{decision:{run_id:"main-run",action:"resume"}});
    expect(invoke.mock.calls.some(([command])=>command==="start_temporary_task")).toBe(false);
  });
  it("steers the active temporary child when the user is reading its paused parent",async()=>{
    const {invoke,send}=fixture({...main,status:"paused"},{parent:null,child});
    await send("再补充一个检查：只修改受影响的子目录");
    expect(invoke).toHaveBeenCalledWith("start_temporary_task",expect.objectContaining({parentRunId:"child-run"}));
  });
  it("returns to the main task through ordinary chat",async()=>{
    const {invoke,send}=fixture(child,{parent:{...main,status:"paused"},child:null});
    expect(await send("恢复主任务")).toEqual({handled:true,conversationId:"main"});
    expect(invoke).toHaveBeenCalledWith("return_to_parent_task",{runId:"child-run"});
  });
  it.each(["补充要求：只分析前端", "继续任务", "暂停任务"])("rejects an unapproved plan before passing the new message to normal sending: %s",async text=>{
    const plan = '{"goal":"原规划"}';
    const {invoke,send}=fixture({...main,status:"awaiting_plan",plan_json:plan});
    expect(await send(text)).toEqual({handled:false});
    expect(invoke).toHaveBeenCalledWith("respond_background_agent",{decision:{run_id:"main-run",action:"reject_plan",answer:plan}});
    expect(invoke.mock.calls.map(([command])=>command)).toEqual(["list_agent_runs","respond_background_agent"]);
  });
  it("does not send the new request when rejecting the old plan fails",async()=>{
    const {invoke,send}=fixture({...main,status:"awaiting_plan"});
    invoke.mockImplementation(async command=>{
      if(command==="list_agent_runs")return [{...main,status:"awaiting_plan"}];
      throw new Error("reject failed");
    });
    await expect(send("修改规划")).rejects.toThrow("reject failed");
    expect(invoke.mock.calls.some(([command])=>command==="start_temporary_task"||command==="append_message")).toBe(false);
  });
  it("leaves a completed conversation available for an ordinary new task",async()=>{
    const {invoke,send}=fixture({...main,status:"completed"});
    expect(await send("帮我写一个新功能")).toEqual({handled:false});
    expect(invoke).toHaveBeenCalledTimes(1);
  });
  it("keeps the new request available to retry when insertion fails",async()=>{
    const {invoke,send}=fixture();
    invoke.mockImplementation(async(command)=>{if(command==="list_agent_runs")return [main];if(command==="get_task_interruption")return {parent:null,child:null};throw new Error("pause failed");});
    await expect(send("先修复临时问题")).rejects.toThrow("pause failed");
    expect(invoke.mock.calls.some(([command])=>command==="append_message")).toBe(false);
  });
});
