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

export function ExtensionDialog({ title, busy, onClose, children, footer, wide = false }: Props) {
  return (
    <ModalOverlay
      isOpen
      isDismissable={!busy}
      isKeyboardDismissDisabled={busy}
      onOpenChange={(open) => {
        if (!open && !busy) onClose();
      }}
      className="asb-ext-modal-backdrop"
    >
      <Modal className={`asb-ext-modal${wide ? " asb-ext-modal-wide" : ""}`}>
        <Dialog aria-label={title} className="asb-ext-dialog">
          <header className="asb-ext-dialog-heading">
            <Heading slot="title" className="asb-panel-title">
              {title}
            </Heading>
            <Button variant="icon" disabled={busy} aria-label="关闭窗口" onClick={onClose}>
              <CloseIcon />
            </Button>
          </header>
          <div className="asb-ext-dialog-body">{children}</div>
          {footer && <footer className="asb-ext-dialog-footer">{footer}</footer>}
        </Dialog>
      </Modal>
    </ModalOverlay>
  );
}
