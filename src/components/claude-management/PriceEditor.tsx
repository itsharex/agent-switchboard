import { useState } from "react";
import * as api from "../../api/claude-ledger";
import { Button } from "../Button";
import { Input } from "../Input";
import { Select } from "../Select";
import type { ClaudeOperations } from "./operations";
const FIELDS = [["inputUsdPerMillion", "输入"], ["outputUsdPerMillion", "输出（含思考）"], ["cacheReadUsdPerMillion", "缓存读取"], ["cacheCreationUsdPerMillion", "缓存写入"]] as const;
const blank = (): api.ClaudeModelPrice => ({ inputUsdPerMillion: "", outputUsdPerMillion: "", cacheReadUsdPerMillion: "", cacheCreationUsdPerMillion: "", source: "user" });
export function PriceEditor({ snapshot, operations: op, onSaved }: {
  snapshot: api.ClaudePriceBookSnapshot; operations: ClaudeOperations; onSaved: (next: api.ClaudePriceBookSnapshot) => void;
}) {
  const [id, setId] = useState("new"); const [model, setModel] = useState(""); const [price, setPrice] = useState<api.ClaudeModelPrice>(blank);
  const choose = (id: string) => { setId(id); setModel(id === "new" ? "" : id); setPrice(id === "new" ? blank() : snapshot.book.models[id]); };
  const save = (remove: boolean) => void op.run(async () => {
    const models = { ...snapshot.book.models }; const name = model.trim();
    if (!name) throw new Error("请填写计价模型 ID");
    if (remove) delete models[name]; else models[name] = price;
    onSaved(await api.setClaudePriceBook({ version: 1, models }, snapshot.fileHash, true));
    choose("new"); op.changed("Claude 价格表已保存；仅用于新记账请求，不改写已记录费用。");
  });
  return <section aria-label="Claude 模型价格表" className="asb-provider-section-fields">
    <h3 className="asb-section-title">模型价格（USD / 百万 token）</h3>
    <Select ariaLabel="Claude 计价模型" value={id} disabled={op.busy} onChange={choose} options={[{ value: "new", label: "新增模型价格" }, ...Object.keys(snapshot.book.models).sort().map((id) => ({ value: id, label: id }))]} />
    <label className="asb-field"><span>计价模型 ID</span><Input value={model} disabled={op.busy || id !== "new"} onChange={(e) => setModel(e.target.value)} /></label>
    <div className="asb-provider-field-grid">{FIELDS.map(([key, label]) => <label className="asb-field" key={key}><span>{label}</span><Input inputMode="decimal" value={price[key]} disabled={op.busy} onChange={(e) => setPrice({ ...price, [key]: e.target.value })} /></label>)}</div>
    <label className="asb-field"><span>价格来源</span><Input value={price.source} disabled={op.busy} onChange={(e) => setPrice({ ...price, source: e.target.value })} /></label>
    <div className="asb-form-actions"><Button variant="primary" disabled={op.busy || !model.trim()} onClick={() => save(false)}>确认保存 Claude 模型价格</Button>
      <Button variant="danger" disabled={op.busy || id === "new"} onClick={() => save(true)}>确认移除此模型价格</Button>
      <Button variant="secondary" disabled={op.busy} onClick={() => void op.run(async () => { onSaved(await api.getClaudePriceBook()); choose("new"); })}>重新读取价格表</Button></div>
  </section>;
}
