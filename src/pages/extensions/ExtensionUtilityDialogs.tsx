import { useState } from "react";
import { AppDialog } from "../../components/AppDialog";
import { ExtensionHistory } from "../../components/extensions/ExtensionHistory";
import { NewMcpForm } from "../../components/extensions/NewMcpForm";
import { NewSkillForm } from "./NewSkillForm";
import { PortableImportForm } from "./PortableImportForm";
import { ProjectRegisterForm } from "./ProjectRegisterForm";
import type { ExtensionWorkspace } from "./useExtensionWorkspace";

type Props = { workspace: ExtensionWorkspace };

export function NewMcpDialog({ workspace: w }: Props) {
  return (
    <AppDialog title="添加 MCP" busy={w.busy} onClose={w.nav.closeDialog}>
      <NewMcpForm
        busy={w.writeBlocked}
        onPutSecret={w.ext.putSecret}
        onSave={w.createMcp}
      />
    </AppDialog>
  );
}

export function NewSkillDialog({ workspace: w }: Props) {
  const [name, setName] = useState("");
  const [description, setDescription] = useState("");
  return (
    <AppDialog title="新建本地 Skill" busy={w.busy} onClose={w.nav.closeDialog}>
      <NewSkillForm
        busy={w.writeBlocked}
        newSkillName={name}
        setNewSkillName={setName}
        newSkillDescription={description}
        setNewSkillDescription={setDescription}
        createSkill={async () => {
          const definition = await w.ext.createSkill({ name: name.trim(), description: description.trim() });
          if (definition) w.nav.openSkillEditor(definition.id);
        }}
      />
    </AppDialog>
  );
}

export function PortableImportDialog({ workspace: w }: Props) {
  const [path, setPath] = useState("");
  return (
    <AppDialog title="导入便携包" busy={w.busy} onClose={w.nav.closeDialog}>
      <PortableImportForm
        busy={w.writeBlocked}
        portablePath={path}
        setPortablePath={setPath}
        submitPortableImport={async () => {
          if (!path.trim()) return;
          const result = await w.ext.importPortable(path.trim());
          if (result) w.nav.closeDialog();
        }}
      />
    </AppDialog>
  );
}

export function ProjectDialog({ workspace: w }: Props) {
  const [root, setRoot] = useState("");
  return (
    <AppDialog title="注册项目目录" busy={w.busy} onClose={w.nav.closeDialog}>
      <ProjectRegisterForm
        busy={w.writeBlocked}
        projectRoot={root}
        setProjectRoot={setRoot}
        submitProject={async () => {
          if (root.trim() && (await w.ext.addProject(root.trim()))) w.nav.closeDialog();
        }}
      />
    </AppDialog>
  );
}

export function HistoryDialog({ workspace: w }: Props) {
  return (
    <AppDialog title="操作历史" busy={w.busy} onClose={w.nav.closeDialog} wide>
      <ExtensionHistory
        records={w.ext.workspace?.history ?? []}
        items={w.items}
        busy={w.writeBlocked}
        projectNames={w.projectNames}
        onRestore={(id) => void w.applies.restore(id)}
      />
    </AppDialog>
  );
}
