import type { ReactNode } from "react";
import { Dialog, Heading, Modal, ModalOverlay } from "react-aria-components";
import { Button } from "../Button";
import { CloseIcon } from "../icons";

interface Props {
  title: string;
  busy: boolean;
  onClose: () => void;
  children: ReactNode;
  footer?: ReactNode;
  wide?: boolean;
}

/**
 * The app's modal wrapper. Structure and geometry belong to the `.asb-dialog*`
 * skeleton in styles/base/overlays.css — this component only supplies React
 * Aria's focus isolation, the title slot and the close affordance, so every
 * dialog in the app lands on one anatomy and one of three widths.
 */
export function ExtensionDialog({ title, busy, onClose, children, footer, wide = false }: Props) {
  return (
    <ModalOverlay
      isOpen
      isDismissable={!busy}
      isKeyboardDismissDisabled={busy}
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
      className="asb-dialog-backdrop"
    >
      <Modal className={`asb-dialog${wide ? " is-wide" : ""}`}>
        <Dialog aria-label={title} className="asb-dialog-surface">
          <header className="asb-dialog-heading">
            <Heading slot="title" className="asb-dialog-title">
              {title}
            </Heading>
            <Button variant="icon" disabled={busy} aria-label="关闭窗口" onClick={onClose}>
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
