import { Button, Modal, Stack, Text } from "@mantine/core";
import type { useAutomationController } from "../../hooks/useAutomationController";
import NaturalAutomationCreator from "./NaturalAutomationCreator";

export type AutomationEditorProps = ReturnType<typeof useAutomationController>["editor"];
export default function AutomationEditor({ editing, ...props }: AutomationEditorProps) {
  return <Modal opened={editing} onClose={props.close} title={props.draft.id ? "编辑任务" : "新建任务"} size="lg" closeButtonProps={{ "aria-label": "关闭任务编辑" }}>
    {props.draft.action.kind === "command" || props.draft.trigger.kind === "files" ? <Stack>
      <Text role="alert">此任务使用旧版脚本或文件监听配置，当前 AI 编辑暂不支持。原任务可继续运行、暂停、立即执行或删除，不会自动转换为文本任务。</Text>
      <Button variant="default" onClick={props.close}>关闭</Button>
    </Stack> : <NaturalAutomationCreator key={props.draft.id || "new"} {...props} />}
  </Modal>;
}
