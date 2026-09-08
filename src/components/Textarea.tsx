import { forwardRef, type TextareaHTMLAttributes } from "react";

interface Props
  extends Omit<TextareaHTMLAttributes<HTMLTextAreaElement>, "className"> {
  /** 等宽代码变体：逐行核对的内容（如可选模型列表）。 */
  code?: boolean;
}

/**
 * Multi-line sibling of Input: native
 * attributes pass through; the field look is shared with .asb-input and the
 * multi-line sizing with .asb-textarea in styles/base.css. The class list is
 * owned exclusively by this component; sizing variants belong to parent
 * containers, not a className pass-through.
 */
export const Textarea = forwardRef<HTMLTextAreaElement, Props>(function Textarea(
  { code = false, ...props },
  ref,
) {
  return (
    <textarea
      ref={ref}
      className={code ? "asb-input asb-code asb-textarea" : "asb-input asb-textarea"}
      {...props}
    />
  );
});
