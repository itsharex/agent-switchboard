import { useRef, type ChangeEvent } from "react";
import { useI18n } from "../i18n";

interface EditableCodePreviewProps {
  target: string;
  content: string;
  disabled: boolean;
  onChange: (content: string) => void;
}

/** A compact source editor for the typed client-settings fragment. The
 * renderer owns parsing; this component only preserves the user's text. */
export function EditableCodePreview({
  target,
  content,
  disabled,
  onChange,
}: EditableCodePreviewProps) {
  const { t } = useI18n();
  const gutter = useRef<HTMLDivElement>(null);
  const lines = content.replace(/\n$/, "").split("\n");
  const handleChange = (event: ChangeEvent<HTMLTextAreaElement>) => {
    onChange(event.target.value);
  };
  return (
    <div className="asb-filepreview asb-fileeditor" aria-label={t("providers.code.editorAria", { target })}>
      <div className="asb-filepreview-head">
        <span className="asb-code">{target}</span>
        <span className="asb-filepreview-meta">{t("providers.code.lines", { count: lines.length })}</span>
      </div>
      <div className="asb-fileeditor-body">
        <div className="asb-fileeditor-gutter" aria-hidden="true">
          <div className="asb-fileeditor-lines" ref={gutter}>
            {lines.map((_, index) => <span className="asb-fileeditor-line" key={index}>{index + 1}</span>)}
          </div>
        </div>
        <textarea
          aria-label={t("providers.code.editorContentAria", { target })}
          className="asb-fileeditor-input"
          disabled={disabled}
          onChange={handleChange}
          onScroll={(event) => {
            if (gutter.current) gutter.current.style.transform = `translateY(-${event.currentTarget.scrollTop}px)`;
          }}
          spellCheck={false}
          value={content}
          wrap="soft"
        />
      </div>
    </div>
  );
}
