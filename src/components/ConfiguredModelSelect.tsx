import { Select, type SelectProps } from "@mantine/core";
import type { ModelConfig } from "../types";
import { buildTaskPriceModelOptions } from "../lib/modelOptions";

type ConfiguredModelSelectProps = Omit<SelectProps, "data"> & { models: ModelConfig[] };

export default function ConfiguredModelSelect({ models, ...props }: ConfiguredModelSelectProps) {
  return <Select {...props} data={buildTaskPriceModelOptions(models)} />;
}
