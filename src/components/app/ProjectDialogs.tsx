import { Button, Group, Modal, Stack, Text, TextInput, ThemeIcon } from "@mantine/core";
import { FolderPlus, Trash2 } from "lucide-react";
import type { UseProjectsReturn } from "../../hooks/useProjects";
export default function ProjectDialogs({ projects }: { projects: UseProjectsReturn }) {
  return (
    <>
      {projects.showNewProjectDialog && (
        <Modal
          opened
          onClose={() => projects.setShowNewProjectDialog(false)}
          size="md"
          title={
            <Group gap="sm">
              <ThemeIcon variant="light" color="teal" size="md">
                <FolderPlus size={18} />
              </ThemeIcon>
              <Text fw={650}>新建项目</Text>
            </Group>
          }
        >
          <Stack gap="md">
            <TextInput
              label="工作目录"
              value={projects.newProjectWorkdir}
              readOnly
              placeholder="选择真实工作目录"
              rightSection={
                <Button
                  variant="subtle"
                  size="compact-sm"
                  onClick={() => void projects.handleSelectNewProjectWorkdir()}
                >
                  选择
                </Button>
              }
              rightSectionWidth={62}
            />
            <TextInput
              label="项目名称"
              value={projects.newProjectName}
              onChange={(event) => projects.setNewProjectName(event.currentTarget.value)}
              placeholder="逻辑名称，例如：官网改版"
              autoFocus
            />
            <Group justify="flex-end" mt="sm">
              <Button variant="default" onClick={() => projects.setShowNewProjectDialog(false)}>
                取消
              </Button>
              <Button
                leftSection={<FolderPlus size={15} />}
                onClick={() => void projects.handleCreateProject()}
              >
                添加并打开
              </Button>
            </Group>
          </Stack>
        </Modal>
      )}

      {projects.pendingProjectRemoval && (
        <Modal
          opened
          onClose={() => projects.setPendingProjectRemoval(null)}
          size="md"
          title={
            <Group gap="sm">
              <ThemeIcon variant="light" color="red" size="md">
                <Trash2 size={18} />
              </ThemeIcon>
              <Text fw={650}>移除项目入口</Text>
            </Group>
          }
        >
          <Stack gap="md">
            <Text size="sm" c="dimmed">
              将从项目区移除 <strong>{projects.pendingProjectRemoval.name}</strong>。此操作不会删除磁盘文件。
            </Text>
            <TextInput
              label="输入项目名称以确认"
              value={projects.projectApprovalText}
              onChange={(event) => projects.setProjectApprovalText(event.currentTarget.value)}
              placeholder={projects.pendingProjectRemoval.name}
              autoFocus
            />
            <Group justify="flex-end" mt="sm">
              <Button variant="default" onClick={() => projects.setPendingProjectRemoval(null)}>
                取消
              </Button>
              <Button
                color="red"
                leftSection={<Trash2 size={15} />}
                onClick={projects.handleConfirmRemoveProject}
                disabled={projects.projectApprovalText.trim() !== projects.pendingProjectRemoval.name}
              >
                批准移除
              </Button>
            </Group>
          </Stack>
        </Modal>
      )}
    </>
  );
}
