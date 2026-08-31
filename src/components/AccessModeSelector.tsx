import { Menu, UnstyledButton } from "@mantine/core";
import { Check, ChevronDown, Hand, ShieldAlert, ShieldCheck } from "lucide-react";
import { ACCESS_MODE_OPTIONS, getAccessModeOption } from "../lib/accessMode";
import type { AgentAccessMode } from "../types";

interface AccessModeSelectorProps {
  value: AgentAccessMode;
  onChange: (value: AgentAccessMode) => void;
  disabled?: boolean;
}

const MODE_ICONS = {
  ask: Hand,
  auto: ShieldCheck,
  full: ShieldAlert
} as const;

export default function AccessModeSelector({ value, onChange, disabled = false }: AccessModeSelectorProps) {
  const activeOption = getAccessModeOption(value);
  const ActiveIcon = MODE_ICONS[value];

  return (
    <Menu
      width={350}
      position="top-start"
      offset={10}
      shadow="xl"
      radius="lg"
      withinPortal
      transitionProps={{ transition: "pop-bottom-left", duration: 140 }}
    >
      <Menu.Target>
        <UnstyledButton
          className={`access-mode-trigger ${value === "full" ? "risk-high" : ""}`}
          aria-label={`应用模式：${activeOption.label}`}
          disabled={disabled}
        >
          <ActiveIcon size={16} strokeWidth={2} />
          <span>{activeOption.label}</span>
          <ChevronDown size={14} className="access-mode-chevron" />
        </UnstyledButton>
      </Menu.Target>

      <Menu.Dropdown className="access-mode-menu" aria-label="选择应用模式">
        {ACCESS_MODE_OPTIONS.map((option) => {
          const Icon = MODE_ICONS[option.value];
          const selected = option.value === value;
          return (
            <Menu.Item
              key={option.value}
              className={`access-mode-option ${option.value === "full" ? "risk-high" : ""}`}
              leftSection={<Icon size={22} strokeWidth={1.8} />}
              rightSection={selected ? <Check size={20} strokeWidth={2.2} /> : null}
              onClick={() => onChange(option.value)}
              disabled={disabled}
            >
              <span className="access-mode-option-copy">
                <strong>{option.label}</strong>
                <small>{option.description}</small>
              </span>
            </Menu.Item>
          );
        })}
      </Menu.Dropdown>
    </Menu>
  );
}
