import { lazy } from "react";

const App = lazy(() => import("./App"));
const AssistantMode = lazy(() => import("./components/AssistantMode"));
const DocumentChat = lazy(() => import("./components/DocumentChat"));

export default function WindowEntry({ search }: { search: string }) {
  const params = new URLSearchParams(search);
  const documentPath = params.get("document-chat");
  if (documentPath !== null) return <DocumentChat path={documentPath} />;
  if (params.has("assistant")) return <AssistantMode />;
  return <App />;
}
