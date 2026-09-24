import { Component, type ErrorInfo, type ReactNode } from "react";
import { useI18n } from "../i18n";

interface Props {
  children: ReactNode;
}

interface State {
  failed: boolean;
}

/** Keeps the desktop shell alive if one workspace surface throws during
    rendering. The title bar and independent tray window remain available. */
export class AppErrorBoundary extends Component<Props, State> {
  state: State = { failed: false };

  static getDerivedStateFromError(): State {
    return { failed: true };
  }

  componentDidCatch(_error: Error, _info: ErrorInfo) {
    // Deliberately avoid logging UI errors here: logs can contain rendered
    // config details. The visible fallback is the recovery surface.
  }

  render() {
    if (this.state.failed) {
      return <RecoveryMessage />;
    }
    return this.props.children;
  }
}

function RecoveryMessage() {
  const { t } = useI18n();
  return (
    <section className="asb-panel asb-recovery-panel" role="alert" aria-label={t("recovery.aria")}>
      <h2 className="asb-recovery-title">{t("recovery.title")}</h2>
      <p className="asb-scope-note">{t("recovery.body")}</p>
    </section>
  );
}
