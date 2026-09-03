// ----------
// React Error Boundary & Crash Recovery
// Description: Top-level error boundary that catches uncaught frontend exceptions or corrupted state, displays a native Clipbox error card with diagnostics, and provides restart, reset, and copy-report actions.
// ----------

import { Component, ErrorInfo, ReactNode } from "react";
import { invoke } from "@tauri-apps/api/core";
import { Titlebar } from "@/components/titlebar";
import { Button } from "@/components/ui/button";
import { AlertTriangle, RotateCcw, Copy, Check, ChevronDown, ChevronUp, RefreshCw, Trash2 } from "lucide-react";

interface Props {
  children: ReactNode;
}

interface State {
  hasError: boolean;
  error: Error | null;
  errorInfo: ErrorInfo | null;
  showDetails: boolean;
  copied: boolean;
}

export class ErrorBoundary extends Component<Props, State> {
  constructor(props: Props) {
    super(props);
    this.state = {
      hasError: false,
      error: null,
      errorInfo: null,
      showDetails: false,
      copied: false,
    };
  }

  static getDerivedStateFromError(error: Error): Partial<State> {
    return { hasError: true, error };
  }

  componentDidCatch(error: Error, errorInfo: ErrorInfo) {
    console.error("Clipbox caught unhandled error in React tree:", error, errorInfo);
    this.setState({ errorInfo });
  }

  componentDidMount() {
    window.addEventListener("unhandledrejection", this.handleUnhandledRejection);
  }

  componentWillUnmount() {
    window.removeEventListener("unhandledrejection", this.handleUnhandledRejection);
  }

  handleUnhandledRejection = (event: PromiseRejectionEvent) => {
    const reason = event.reason;
    const reasonStr = String(reason?.message || reason || "");

    // Ignore benign non-fatal events (such as window drag release or abort errors)
    if (
      reasonStr.includes("start_dragging") ||
      reasonStr.includes("cancelled") ||
      reasonStr.includes("AbortError")
    ) {
      return;
    }

    const error =
      reason instanceof Error
        ? reason
        : new Error(String(reason || "Unhandled Promise Rejection"));
    console.error("Unhandled promise rejection in window:", error);
    this.setState({
      hasError: true,
      error,
    });
  };

  handleRestart = async () => {
    try {
      if (typeof window !== "undefined" && "__TAURI_INTERNALS__" in window) {
        await invoke("restart_app");
        return;
      }
    } catch (e) {
      console.warn("Could not invoke restart_app, reloading window", e);
    }
    window.location.reload();
  };

  handleResetAndRestart = async () => {
    try {
      localStorage.clear();
      sessionStorage.clear();
    } catch {
      // ignore
    }
    await this.handleRestart();
  };

  handleTryAgain = () => {
    this.setState({
      hasError: false,
      error: null,
      errorInfo: null,
      showDetails: false,
      copied: false,
    });
  };

  handleCopyDetails = async () => {
    const payload = {
      timestamp: new Date().toISOString(),
      userAgent: navigator.userAgent,
      error: {
        name: this.state.error?.name,
        message: this.state.error?.message,
        stack: this.state.error?.stack,
      },
      componentStack: this.state.errorInfo?.componentStack,
    };

    try {
      await navigator.clipboard.writeText(JSON.stringify(payload, null, 2));
      this.setState({ copied: true });
      setTimeout(() => this.setState({ copied: false }), 2000);
    } catch (err) {
      console.warn("Failed to copy error report", err);
    }
  };

  render() {
    if (!this.state.hasError) {
      return this.props.children;
    }

    const { error, errorInfo, showDetails, copied } = this.state;

    return (
      <div className="h-screen w-screen bg-background text-foreground flex flex-col font-sans select-none overflow-hidden">
        {/* Titlebar with window controls intact */}
        <Titlebar />

        {/* Error Screen Content */}
        <div className="flex-1 flex items-center justify-center p-6 overflow-y-auto">
          <div className="max-w-md w-full bg-card border rounded-xl shadow-lg p-6 flex flex-col items-center text-center space-y-4">
            {/* Warning Icon with subtle glow */}
            <div className="size-14 rounded-full bg-destructive/10 border border-destructive/20 flex items-center justify-center text-destructive shadow-sm">
              <AlertTriangle className="size-7" />
            </div>

            {/* Error Title and Subtitle */}
            <div className="space-y-1">
              <h2 className="text-xl font-bold font-dmsans tracking-tight">Something went wrong</h2>
              <p className="text-xs text-muted-foreground">
                Clipbox encountered an unexpected problem and had to stop.
              </p>
            </div>

            {/* Error Message Box */}
            {error && (
              <div className="w-full bg-muted/40 border border-border/70 rounded-lg p-3 text-left">
                <div className="text-[11px] font-mono text-muted-foreground uppercase tracking-wider mb-1 font-semibold">
                  Reason
                </div>
                <div className="text-xs font-mono text-destructive break-words select-text">
                  {error.name ? `${error.name}: ` : ""}{error.message || "Unknown error occurred"}
                </div>
              </div>
            )}

            {/* Technical Details Toggle */}
            <div className="w-full text-left">
              <button
                type="button"
                onClick={() => this.setState({ showDetails: !showDetails })}
                className="flex items-center justify-between w-full py-1 text-xs text-muted-foreground hover:text-foreground transition-colors font-medium cursor-pointer"
              >
                <span>Technical details</span>
                {showDetails ? <ChevronUp className="size-3.5" /> : <ChevronDown className="size-3.5" />}
              </button>

              {showDetails && (
                <div className="mt-2 space-y-2">
                  <pre className="p-3 bg-muted/70 rounded-md text-[11px] font-mono text-muted-foreground overflow-x-auto max-h-40 select-text leading-relaxed whitespace-pre-wrap break-all">
                    {error?.stack || errorInfo?.componentStack || "No stack trace available."}
                  </pre>
                  <Button
                    variant="outline"
                    size="sm"
                    onClick={this.handleCopyDetails}
                    className="h-7 text-xs gap-1.5 w-full cursor-pointer"
                  >
                    {copied ? <Check className="size-3 text-green-500" /> : <Copy className="size-3" />}
                    {copied ? "Copied diagnostic report!" : "Copy diagnostic report"}
                  </Button>
                </div>
              )}
            </div>

            {/* Action Buttons */}
            <div className="w-full pt-2 flex flex-col gap-2">
              <Button
                onClick={this.handleRestart}
                className="w-full gap-2 h-9 text-sm font-medium cursor-pointer"
              >
                <RotateCcw className="size-4" />
                Restart Application
              </Button>

              <div className="flex gap-2 w-full">
                <Button
                  variant="outline"
                  onClick={this.handleTryAgain}
                  className="flex-1 gap-1.5 h-8 text-xs cursor-pointer"
                >
                  <RefreshCw className="size-3" />
                  Try Again
                </Button>
                <Button
                  variant="ghost"
                  onClick={this.handleResetAndRestart}
                  className="flex-1 gap-1.5 h-8 text-xs text-muted-foreground hover:text-destructive cursor-pointer"
                  title="Clears local cache/state and restarts"
                >
                  <Trash2 className="size-3" />
                  Reset & Restart
                </Button>
              </div>
            </div>
          </div>
        </div>
      </div>
    );
  }
}
