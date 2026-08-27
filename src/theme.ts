import { createTheme, type MantineColorsTuple } from "@mantine/core";

const nanoBlue: MantineColorsTuple = [
  "#edf8ff",
  "#d7efff",
  "#adddff",
  "#7cc9ff",
  "#55b8ff",
  "#3aafff",
  "#269df2",
  "#1689da",
  "#0878c4",
  "#0068ae"
];

export const nanoTheme = createTheme({
  primaryColor: "nanoBlue",
  colors: {
    nanoBlue
  },
  defaultRadius: "md",
  fontFamily: '"Manrope", "Noto Sans SC", "Microsoft YaHei", sans-serif',
  headings: {
    fontFamily: '"Manrope", "Noto Sans SC", "Microsoft YaHei", sans-serif',
    fontWeight: "650"
  },
  cursorType: "pointer",
  components: {
    Button: {
      defaultProps: {
        radius: "md",
        size: "sm"
      }
    },
    ActionIcon: {
      defaultProps: {
        radius: "md",
        size: "md",
        variant: "subtle",
        color: "gray"
      }
    },
    Modal: {
      defaultProps: {
        radius: "lg",
        centered: true,
        overlayProps: {
          backgroundOpacity: 0.48,
          blur: 2
        }
      }
    },
    Tooltip: {
      defaultProps: {
        radius: "md",
        openDelay: 300,
        closeDelay: 120,
        withArrow: true
      },
      styles: {
        tooltip: {
          backgroundColor: "var(--tooltip-bg)",
          color: "var(--tooltip-color)",
          border: "1px solid var(--tooltip-border)",
          boxShadow: "var(--tooltip-shadow)",
          fontSize: "var(--tooltip-font-size)",
          fontWeight: "var(--font-weight-medium)",
          lineHeight: 1.55,
          letterSpacing: "0.01em",
          padding: "6px 11px"
        }
      }
    }
  }
});
