import { createContext, useContext, type ReactNode } from "react";
import { Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { useI18n } from "../i18n";
import { Button } from "./Button";
import { CloseIcon } from "./icons";

interface Props {
  title: string;
  busy: boolean;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
}

const AppDialogSuspensionContext = createContext(false);

export function AppDialogSuspension({ suspended, children }: {
  suspended: boolean;
  children: ReactNode;
}) {
  return (
    <AppDialogSuspensionContext.Provider value={suspended}>
      {children}
    </AppDialogSuspensionContext.Provider>
  );
}

/**
 * The app's modal wrapper. Structure and geometry belong to the `.asb-dialog*`
 * skeleton in styles/base/overlays.css — this component only supplies React
 * Aria's focus isolation, the title slot and the close affordance, so every
 * dialog in the app lands on one anatomy and one of three widths.
 */
export function AppDialog({ title, busy, onClose, children, footer, wide = false }: Props) {
  const suspended = useContext(AppDialogSuspensionContext);
  const { t } = useI18n();
  return (
    <ModalOverlay
      isOpen
      isDismissable={!busy}
      isKeyboardDismissDisabled={busy}
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
      aria-hidden={suspended || undefined}
      className={`asb-dialog-backdrop${suspended ? " is-suspended" : ""}`}
    >
      <Modal className={`asb-dialog${wide ? " is-wide" : ""}`}>
        <Dialog aria-label={title} className="asb-dialog-surface">
          <header className="asb-dialog-heading">
            <Heading slot="title" className="asb-dialog-title">
              {title}
            </Heading>
            <Button variant="icon" disabled={busy} aria-label={t("window.closeDialog")} onClick={onClose}>
              <CloseIcon />
            </Button>
          </header>
          <div className="asb-dialog-body">{children}</div>
          {footer && <footer className="asb-dialog-footer">{footer}</footer>}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
