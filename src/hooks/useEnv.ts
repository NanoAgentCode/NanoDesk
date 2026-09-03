import { useEffect, useState } from "react";
import {
  checkEnv,
  installEnv,
  getTavilyApiKey,
  saveTavilyApiKey
} from "../api";
import { APP_NAME, APP_STORAGE_PREFIX } from "../config/brand";

const NODE_PATH_KEY = `${APP_STORAGE_PREFIX}-node-path`;
const PYTHON_PATH_KEY = `${APP_STORAGE_PREFIX}-python-path`;
const ENV_CHECKED_KEY = `${APP_STORAGE_PREFIX}-env-checked`;

export interface UseEnvReturn {
  nodePath: string;
  setNodePath: (value: string) => void;
  pythonPath: string;
  setPythonPath: (value: string) => void;
  tavilyApiKey: string;
  setTavilyApiKey: (value: string) => void;
  envStatus: Record<string, boolean>;
  showCustomPaths: boolean;
  setShowCustomPaths: (value: boolean | ((prev: boolean) => boolean)) => void;
  showEnvActionsMenu: boolean;
  setShowEnvActionsMenu: (value: boolean | ((prev: boolean) => boolean)) => void;
  showEnvPrompt: boolean;
  dismissEnvPrompt: () => void;
  isCheckingEnv: boolean;
  isInstallingEnv: boolean;
  envInstallProgress: string;
  isSavingTavilyApiKey: boolean;
  runEnvCheck: () => Promise<Record<string, boolean>>;
  handleSaveTavilyApiKey: () => Promise<void>;
  handleInstallTavilyCli: () => Promise<void>;
  handleInstallPaddleOcr: () => Promise<void>;
  handleAutoInstallMissing: () => Promise<void>;
  handleSaveCustomPaths: () => Promise<void>;
}

export function useEnv(setNotice: (message: string) => void): UseEnvReturn {
  const [nodePath, setNodePath] = useState(
    () => localStorage.getItem(NODE_PATH_KEY) || ""
  );
  const [pythonPath, setPythonPath] = useState(
    () => localStorage.getItem(PYTHON_PATH_KEY) || ""
  );
  const [tavilyApiKey, setTavilyApiKey] = useState("");
  const [isSavingTavilyApiKey, setIsSavingTavilyApiKey] = useState(false);
  const [envStatus, setEnvStatus] = useState<Record<string, boolean>>({
    node: true,
    python: true,
    paddleocr: false
  });
  const [showCustomPaths, setShowCustomPaths] = useState(false);
  const [showEnvActionsMenu, setShowEnvActionsMenu] = useState(false);
  const [showEnvPrompt, setShowEnvPrompt] = useState(false);
  const [isCheckingEnv, setIsCheckingEnv] = useState(false);
  const [isInstallingEnv, setIsInstallingEnv] = useState(false);
  const [envInstallProgress, setEnvInstallProgress] = useState("");

  useEffect(() => {
    getTavilyApiKey()
      .then((apiKey) => setTavilyApiKey(apiKey))
      .catch((error) => console.error("Failed to load Tavily API key:", error));

    const timer = window.setTimeout(() => {
      const isEnvChecked = localStorage.getItem(ENV_CHECKED_KEY) === "true";

      setIsCheckingEnv(true);
      checkEnv(nodePath, pythonPath)
        .then((status) => {
          setEnvStatus(status);
          if (!isEnvChecked && (!status.node || !status.python)) {
            setShowEnvPrompt(true);
          } else if (!isEnvChecked) {
            localStorage.setItem(ENV_CHECKED_KEY, "true");
          }
        })
        .catch((e) => {
          console.error("Failed to run startup environment check:", e);
        })
        .finally(() => {
          setIsCheckingEnv(false);
        });
    }, 800);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, []);

  function dismissEnvPrompt() {
    localStorage.setItem(ENV_CHECKED_KEY, "true");
    setShowEnvPrompt(false);
  }

  async function handleSaveTavilyApiKey() {
    setIsSavingTavilyApiKey(true);
    try {
      await saveTavilyApiKey(tavilyApiKey);
      setTavilyApiKey(tavilyApiKey.trim());
      if (tavilyApiKey.trim() && envStatus.tavily_cli === false) {
        setNotice("Tavily API Key 已保存，但未检测到 Tavily CLI，请先安装 tavily-cli。");
      } else {
        setNotice(tavilyApiKey.trim() ? "Tavily API Key 已保存。" : "Tavily API Key 已清空。");
      }
    } catch (error) {
      console.error("Failed to save Tavily API key:", error);
      setNotice(`保存 Tavily API Key 失败：${String(error)}`);
    } finally {
      setIsSavingTavilyApiKey(false);
    }
  }

  async function runEnvCheck() {
    setIsCheckingEnv(true);
    try {
      const status = await checkEnv(nodePath, pythonPath);
      setEnvStatus(status);
      return status;
    } catch (e) {
      console.error("Failed to check environment:", e);
      return { node: false, python: false, tavily_cli: false, paddleocr: false };
    } finally {
      setIsCheckingEnv(false);
    }
  }

  async function handleInstallTavilyCli() {
    setIsInstallingEnv(true);
    setEnvInstallProgress("正在安装 Tavily CLI...");
    try {
      const ok = await installEnv("tavily");
      if (!ok) {
        throw new Error("Tavily CLI 安装失败");
      }

      setEnvInstallProgress("安装完成，正在验证 Tavily CLI...");
      const finalStatus = await checkEnv(nodePath, pythonPath);
      setEnvStatus(finalStatus);
      if (finalStatus.tavily_cli) {
        setNotice("Tavily CLI 安装成功。");
      } else {
        setNotice(`Tavily CLI 已尝试安装，但当前 PATH 仍未检测到 tvly。请重启 ${APP_NAME} 或检查 Python Scripts/uv tool 目录是否在 PATH。`);
      }
    } catch (error) {
      console.error("Tavily CLI installation failed:", error);
      setNotice(`Tavily CLI 安装失败：${String(error)}`);
    } finally {
      setIsInstallingEnv(false);
      setEnvInstallProgress("");
    }
  }

  async function handleInstallPaddleOcr() {
    setIsInstallingEnv(true);
    setEnvInstallProgress("正在安装 PaddleOCR 与 PaddlePaddle...");
    try {
      const ok = await installEnv("paddleocr");
      if (!ok) {
        throw new Error("PaddleOCR 安装失败");
      }

      setEnvInstallProgress("安装完成，正在验证 PaddleOCR...");
      const finalStatus = await checkEnv(nodePath, pythonPath);
      setEnvStatus(finalStatus);
      if (finalStatus.paddleocr) {
        setNotice("PaddleOCR 已安装。首次执行 OCR 时会按需准备 PP-OCRv6 small 模型。");
      } else {
        setNotice("PaddleOCR 已尝试安装，但仍未检测到 paddleocr CLI。请重新检测环境，或在启动前设置 NANO_AGENT_PADDLEOCR_BIN。");
      }
    } catch (error) {
      console.error("PaddleOCR installation failed:", error);
      setNotice(`PaddleOCR 安装失败：${String(error)}`);
    } finally {
      setIsInstallingEnv(false);
      setEnvInstallProgress("");
    }
  }

  async function handleAutoInstallMissing() {
    setIsInstallingEnv(true);
    setEnvInstallProgress("正在准备安装环境...");
    try {
      const status = await checkEnv(nodePath, pythonPath);
      if (!status.node) {
        setEnvInstallProgress("正在静默安装 Node.js，这可能需要 1-3 分钟，请稍候...");
        const ok = await installEnv("node");
        if (!ok) {
          throw new Error("Node.js 安装失败");
        }
      }
      if (!status.python) {
        setEnvInstallProgress("正在静默安装 Python 3，这可能需要 1-3 分钟，请稍候...");
        const ok = await installEnv("python");
        if (!ok) {
          throw new Error("Python 3 安装失败");
        }
      }

      setEnvInstallProgress("安装完成！正在验证环境...");
      const finalStatus = await checkEnv(nodePath, pythonPath);
      setEnvStatus(finalStatus);

      if (finalStatus.node && finalStatus.python) {
        setNotice("环境自动配置成功！");
        localStorage.setItem(ENV_CHECKED_KEY, "true");
        setShowEnvPrompt(false);
      } else {
        let errMsg = "部分环境未成功配置：";
        if (!finalStatus.node) errMsg += "Node.js ";
        if (!finalStatus.python) errMsg += "Python ";
        setNotice(errMsg + "。您也可以选择配置已有路径。");
      }
    } catch (e) {
      console.error("Environment installation failed:", e);
      setNotice(`环境自动安装失败: ${String(e)}。请尝试手动配置已有路径。`);
    } finally {
      setIsInstallingEnv(false);
      setEnvInstallProgress("");
    }
  }

  async function handleSaveCustomPaths() {
    localStorage.setItem(NODE_PATH_KEY, nodePath);
    localStorage.setItem(PYTHON_PATH_KEY, pythonPath);

    setIsCheckingEnv(true);
    try {
      const status = await checkEnv(nodePath, pythonPath);
      setEnvStatus(status);
      if (status.node && status.python) {
        localStorage.setItem(ENV_CHECKED_KEY, "true");
        setShowEnvPrompt(false);
        setNotice("环境路径验证通过并保存成功！");
      } else {
        let msg = "已保存，但检测到：";
        if (!status.node) msg += "Node.js 路径无效或未找到；";
        if (!status.python) msg += "Python 路径无效或未找到；";
        setNotice(msg + "请重新确认路径。");
      }
    } catch (e) {
      setNotice(`路径检测失败: ${String(e)}`);
    } finally {
      setIsCheckingEnv(false);
    }
  }

  return {
    nodePath,
    setNodePath,
    pythonPath,
    setPythonPath,
    tavilyApiKey,
    setTavilyApiKey,
    envStatus,
    showCustomPaths,
    setShowCustomPaths,
    showEnvActionsMenu,
    setShowEnvActionsMenu,
    showEnvPrompt,
    dismissEnvPrompt,
    isCheckingEnv,
    isInstallingEnv,
    envInstallProgress,
    isSavingTavilyApiKey,
    runEnvCheck,
    handleSaveTavilyApiKey,
    handleInstallTavilyCli,
    handleInstallPaddleOcr,
    handleAutoInstallMissing,
    handleSaveCustomPaths
  };
}
