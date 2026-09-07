import type { ExtensionKind } from "../../api/client";
import type { AddMode } from "./list-filters";

interface Props {
  kindTab: ExtensionKind;
  addMode: AddMode;
  setAddMode: (mode: AddMode) => void;
}

/** The entry cards that expand the matching add-mode form below them. */
export function AddExtensionCards({ kindTab, addMode, setAddMode }: Props) {
  return (
    <div className="asb-ext-cards">
      {kindTab === "skill" ? (
        <>
          <button
            type="button"
            className="asb-ext-card"
            aria-expanded={addMode === "skillSource"}
            onClick={() => setAddMode(addMode === "skillSource" ? null : "skillSource")}
          >
            <strong>添加 Skill 来源</strong>
            <span>扫描本地目录或显式解析 GitHub 来源，再选择内容版本入库</span>
          </button>
          <button
            type="button"
            className="asb-ext-card"
            aria-expanded={addMode === "newSkill"}
            onClick={() => setAddMode(addMode === "newSkill" ? null : "newSkill")}
          >
            <strong>新建本地 Skill</strong>
            <span>从模板创建可编辑的本地 Skill，保存生成不可变内容版本</span>
          </button>
        </>
      ) : (
        <button
          type="button"
          className="asb-ext-card"
          aria-expanded={addMode === "newMcp"}
          onClick={() => {
            setAddMode(addMode === "newMcp" ? null : "newMcp");
          }}
        >
          <strong>新建 MCP</strong>
          <span>手动填写 stdio 或 HTTP 服务配置并存入扩展库</span>
        </button>
      )}
      <button
        type="button"
        className="asb-ext-card"
        aria-expanded={addMode === "discover"}
        onClick={() => setAddMode(addMode === "discover" ? null : "discover")}
      >
        <strong>从本机发现</strong>
        <span>只读扫描两个客户端已有的 Skill 与 MCP 服务</span>
      </button>
    </div>
  );
}
