import type { InputHTMLAttributes } from "react";

interface Props extends Omit<InputHTMLAttributes<HTMLInputElement>, "size" | "className"> {
  /** 等宽代码变体：模型 ID、环境变量名等逐字符核对的内容。 */
  code?: boolean;
}

/**
 * Text-field control: native attributes
 * pass straight through while the field look (hairline border, control radius,
 * canvas fill, placeholder, hover, disabled) is owned by
 * .asb-input in styles/base.css; every value comes from styles/tokens.css.
 * 字段外观类由本组件独占；不接受 className，尺寸与布局属于父级容器。
 */
export function Input({ code = false, ...props }: Props) {
  return <input className={code ? "asb-input asb-code" : "asb-input"} {...props} />;
}
