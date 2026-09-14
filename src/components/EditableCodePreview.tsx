import { useRef, type ChangeEvent } from "react";

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
  const gutter = useRef<HTMLDivElement>(null);
  const lines = content.replace(/\n$/, "").split("\n");
  const handleChange = (event: ChangeEvent<HTMLTextAreaElement>) => {
    onChange(event.target.value);
  };
  return (
    <div className="asb-filepreview asb-fileeditor" aria-label={`${target} 配置编辑器`}>
      <div className="asb-filepreview-head">
        <span className="asb-code">{target}</span>
        <span className="asb-filepreview-meta">{lines.length} 行</span>
      </div>
      <div className="asb-fileeditor-body">
        <div className="asb-fileeditor-gutter" aria-hidden="true">
          <div className="asb-fileeditor-lines" ref={gutter}>
            {lines.map((_, index) => <span className="asb-fileeditor-line" key={index}>{index + 1}</span>)}
          </div>
        </div>
        <textarea
          aria-label={`${target} 配置编辑器内容`}
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
