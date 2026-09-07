import { useEffect, useRef, useState } from "react";
import { fetchProviderModels, getGatewayStatus } from "../api/client";
import type {
  AppKind,
  ProviderDraft,
  ProviderModel,
  ProviderProfile,
  UpstreamProtocol,
} from "../api/client";
import { clientName } from "../lib/client-name";
import { NATIVE_PROTOCOL, PROTOCOL_LABELS } from "../lib/protocol";
import { ClientLogo } from "./ClientLogo";
import { EyeOffIcon, PreviewIcon } from "./icons";
import { Input } from "./Input";
import { OfficialLoginPanel } from "./OfficialLoginPanel";
import { RadioOption } from "./RadioOption";
import { Button } from "./Button";
import { Select } from "./Select";
import { Textarea } from "./Textarea";
import { normalizeUsageQuery } from "../lib/usage-query";
import { ClaudeModelMapping } from "./provider-editor/ClaudeModelMapping";
import { MainModelField } from "./provider-editor/MainModelField";
import {
  PROTOCOL_AUTHENTICATION_NOTES,
  PROTOCOL_NOTES,
  codexOptionsAreEmpty,
  defaultConnection,
  draftFrom,
  optional,
} from "./provider-editor/draft";

interface Props {
  profile: ProviderProfile | null;
  initialApp: AppKind;
  busy: boolean;
  /** Clients that already own their single official profile. */
  officialTakenApps: AppKind[];
  /** Opens the client's existing official profile for a re-login. */
  onOpenOfficial: (app: AppKind) => void;
  /** Model read from the client's user-level configuration, shown for context
   * while editing; null means the client selects its default model. */
  userConfigModel: string | null;
  /** Known conditions that can override the user-level configuration. */
  userConfigWarnings: string[];
  onSave: (draft: ProviderDraft) => void;
  onCancel: () => void;
}

// Reasoning effort, summary, and verbosity live on the general-settings
// page, not on profiles.

/** The local profile editor; it never edits client configuration directly. */
export function ProviderEditor({
  profile,
  initialApp,
  busy,
  officialTakenApps,
  onOpenOfficial,
  userConfigModel,
  userConfigWarnings,
  onSave,
  onCancel,
}: Props) {
  const [draft, setDraft] = useState<ProviderDraft>(() => draftFrom(profile, initialApp));
  const [models, setModels] = useState<ProviderModel[] | null>(null);
  const [modelsBusy, setModelsBusy] = useState(false);
  const [modelsError, setModelsError] = useState<string | null>(null);
  const [gatewayBaseUrl, setGatewayBaseUrl] = useState<string | null>(null);
  const [gatewayAddressError, setGatewayAddressError] = useState(false);
  const [apiKeyVisible, setApiKeyVisible] = useState(false);
  /** True once the official login panel reported a completed login. */
  const [loginDone, setLoginDone] = useState(false);
  const modelsVersion = useRef(0);
  const baseUrl = draft.baseUrl?.trim() ?? "";
  const codex = draft.app === "codex";
  const official = draft.routeMode === "official";
  const routesThroughGateway =
    !official &&
    draft.upstreamProtocol !== null &&
    draft.upstreamProtocol !== NATIVE_PROTOCOL[draft.app];

  useEffect(() => {
    setDraft(draftFrom(profile, initialApp));
    setApiKeyVisible(false);
    setLoginDone(false);
  }, [profile?.id, initialApp]);

  useEffect(() => {
    modelsVersion.current += 1;
    setModels(null);
    setModelsError(null);
    setModelsBusy(false);
  }, [baseUrl, draft.upstreamProtocol]);

  useEffect(() => {
    if (!routesThroughGateway) {
      setGatewayBaseUrl(null);
      setGatewayAddressError(false);
      return;
    }

    let active = true;
    setGatewayBaseUrl(null);
    setGatewayAddressError(false);
    void (async () => {
      try {
        const status = await getGatewayStatus();
        if (!active) return;
        // A saved port must never be presented as a listening one: without a
        // live listener the editor refuses to show any loopback address.
        setGatewayBaseUrl(status.baseUrl);
        setGatewayAddressError(status.baseUrl === null);
      } catch {
        if (active) setGatewayAddressError(true);
      }
    })();
    return () => {
      active = false;
    };
  }, [routesThroughGateway]);

  const fetchModels = async () => {
    if (
      modelsBusy ||
      !baseUrl ||
      !draft.upstreamProtocol
    ) return;
    const version = modelsVersion.current;
    setModelsBusy(true);
    setModelsError(null);
    try {
      const fetched = await fetchProviderModels(
        baseUrl,
        draft.apiKey,
        draft.upstreamProtocol,
      );
      if (modelsVersion.current === version) setModels(fetched);
    } catch (caught) {
      if (modelsVersion.current === version) {
        setModelsError((caught as { message?: string }).message ?? "无法获取模型列表");
      }
    } finally {
      if (modelsVersion.current === version) setModelsBusy(false);
    }
  };

  const codexSettings = draft.modelOptions?.kind === "codex" ? draft.modelOptions : null;
  const claudeSettings = draft.modelOptions?.kind === "claude" ? draft.modelOptions : null;
  const gatewayRouteWarning = gatewayBaseUrl
    ? `与 ${clientName(draft.app)} 原生协议（${PROTOCOL_LABELS[NATIVE_PROTOCOL[draft.app]]}）不同：切换到该供应商时，客户端的服务地址会被改写为本机协议网关 ${gatewayBaseUrl}（仅监听本机），请求由网关转换为该协议后转发到所填服务地址；请保持本应用运行，退出前先切换到直连或官方登录。`
      : gatewayAddressError
        ? `与 ${clientName(draft.app)} 原生协议（${PROTOCOL_LABELS[NATIVE_PROTOCOL[draft.app]]}）不同：此路径需要本机协议网关转换，但网关当前未在监听。请在“网关”页重试监听或修改端口，再切换到该供应商。`
        : `与 ${clientName(draft.app)} 原生协议（${PROTOCOL_LABELS[NATIVE_PROTOCOL[draft.app]]}）不同：此路径需要本机协议网关转换，正在读取实际监听地址。`;

  return (
    <form
      className="asb-form"
      aria-label={profile ? "编辑供应商" : "新建供应商"}
      onSubmit={(event) => {
        event.preventDefault();
        const modelOptions = codexOptionsAreEmpty(draft.modelOptions) ? null : draft.modelOptions;
        onSave({
          ...draft,
          name: draft.name.trim(),
          model: optional(draft.model ?? ""),
          baseUrl: optional(draft.baseUrl ?? ""),
          apiKey: draft.apiKey.trim(),
          notes: optional(draft.notes ?? ""),
          websiteUrl: optional(draft.websiteUrl ?? ""),
          modelOptions,
          usageQuery: normalizeUsageQuery(draft.usageQuery),
        });
      }}
    >
      <label className="asb-field">
        <span>客户端</span>
        <div className="asb-client-control">
          <ClientLogo app={draft.app} className="asb-edit-logo" />
          <Select
            ariaLabel="客户端"
            value={draft.app}
            options={[
              { value: "codex", label: "Codex" },
              { value: "claude", label: "Claude" },
            ]}
            disabled={Boolean(profile) || busy}
            onChange={(app) => {
              const nextApp = app as AppKind;
              if (nextApp === draft.app) return;
              setDraft(draftFrom(null, nextApp));
              setApiKeyVisible(false);
              setLoginDone(false);
            }}
          />
        </div>
      </label>
      {!profile && (
        <div className="asb-field">
          <span>接入方式</span>
          <div className="asb-segments" role="radiogroup" aria-label="接入方式">
            <RadioOption
              name="access-mode"
              checked={!official}
              disabled={busy}
              label="自定义 API 中继"
              onChange={() => {
                setDraft((current) => ({
                  ...current,
                  routeMode: "custom",
                  ...defaultConnection(current.app),
                }));
                setLoginDone(false);
              }}
            />
            <RadioOption
              name="access-mode"
              checked={official}
              disabled={busy}
              label="官方登录"
              onChange={() => {
                if (officialTakenApps.includes(draft.app)) {
                  onOpenOfficial(draft.app);
                  return;
                }
                // The official contract is credential-free, so the custom
                // fields are dropped the moment the mode is chosen instead of
                // being carried hidden and stripped at submit time.
                setDraft((current) => ({
                  ...current,
                  routeMode: "official",
                  name: current.name.trim() || `${clientName(current.app)} 官方登录`,
                  model: null,
                  baseUrl: null,
                  apiKey: "",
                  upstreamProtocol: null,
                  maxOutputTokens: null,
                  modelOptions: null,
                  usageQuery: null,
                  officialQuotaRefreshIntervalMinutes: null,
                }));
                setLoginDone(false);
              }}
            />
          </div>
        </div>
      )}
      <label className="asb-field">
        <span>名称</span>
        <Input
          value={draft.name}
          required
          disabled={busy}
          onChange={(event) => setDraft((current) => ({ ...current, name: event.target.value }))}
        />
      </label>
      <label className="asb-field">
        <span>官网地址</span>
        <Input
          type="url"
          value={draft.websiteUrl ?? ""}
          disabled={busy}
          placeholder="（可选）"
          onChange={(event) =>
            setDraft((current) => ({ ...current, websiteUrl: event.target.value }))
          }
        />
      </label>
      <label className="asb-field">
        <span>备注</span>
        <Textarea
          rows={2}
          value={draft.notes ?? ""}
          disabled={busy}
          placeholder="（可选，仅保存在本应用）"
          onChange={(event) => setDraft((current) => ({ ...current, notes: event.target.value }))}
        />
      </label>
      {!official && (
      <label className="asb-field">
        <span>API 格式</span>
        <Select
          ariaLabel="API 格式"
          value={draft.upstreamProtocol}
          options={[
            { value: "anthropicMessages", label: "Anthropic Messages (/v1/messages)" },
            { value: "chatCompletions", label: "Chat Completions (/v1/chat/completions)" },
            { value: "responses", label: "Responses (/v1/responses)" },
          ]}
          disabled={busy}
          onChange={(value) => {
            const upstreamProtocol = value as UpstreamProtocol;
            setDraft((current) => ({
              ...current,
              upstreamProtocol,
              maxOutputTokens:
                current.app === "codex" && upstreamProtocol === "anthropicMessages"
                  ? (current.maxOutputTokens ?? 8192)
                  : null,
            }));
          }}
        />
        {draft.upstreamProtocol && (
          <>
            <p className="asb-scope-note">{PROTOCOL_NOTES[draft.upstreamProtocol]}</p>
            <p className="asb-scope-note">
              {PROTOCOL_AUTHENTICATION_NOTES[draft.upstreamProtocol]}
            </p>
          </>
        )}
        {draft.upstreamProtocol === NATIVE_PROTOCOL[draft.app] ? (
          <p className="asb-scope-note">
            {`与 ${clientName(draft.app)} 原生协议一致，切换后客户端直连所填服务地址。`}
          </p>
        ) : (
          <p className="asb-scope-note asb-warn-text">
            {gatewayRouteWarning}
          </p>
        )}
        {codex && draft.upstreamProtocol !== "responses" && (
          <p className="asb-scope-note asb-warn-text">
            {`此路由下 Codex 的网页搜索会关闭，client_metadata、prompt_cache_key、reasoning.summary=auto 与 reasoning.encrypted_content 也不会转发到上游；需要这些能力请使用 Responses 上游。`}
          </p>
        )}
      </label>
      )}
      {!official && draft.app === "codex" && draft.upstreamProtocol === "anthropicMessages" && (
      <label className="asb-field">
        <span>最大输出 Token</span>
        <Input
          aria-label="最大输出 Token"
          type="number"
          min="1"
          step="1"
          required
          value={draft.maxOutputTokens?.toString() ?? ""}
          disabled={busy}
          onChange={(event) => {
            const value = event.target.value.trim();
            const parsed = Number(value);
            setDraft((current) => ({
              ...current,
              maxOutputTokens:
                value && Number.isSafeInteger(parsed) && parsed > 0 ? parsed : null,
            }));
          }}
        />
        <p className="asb-scope-note">
          Codex 未发送单次上限时，网关将使用此值构造 Anthropic 请求。
        </p>
      </label>
      )}
      {!official && (
      <div className="asb-field">
        <span>服务地址</span>
        <Input
          aria-label="服务地址"
          type="url"
          required
          value={draft.baseUrl ?? ""}
          disabled={busy}
          onChange={(event) => setDraft((current) => ({ ...current, baseUrl: event.target.value }))}
        />
      </div>
      )}
      {!official && (
      <div className="asb-field">
        <span>API 密钥</span>
        <div className="asb-secret-control">
          <Input
            aria-label="API 密钥"
            type={apiKeyVisible ? "text" : "password"}
            required
            value={draft.apiKey}
            disabled={busy}
            onChange={(event) =>
              setDraft((current) => ({ ...current, apiKey: event.target.value }))
            }
          />
          <Button
            variant="secondary"
            aria-pressed={apiKeyVisible}
            disabled={busy}
            onClick={() => setApiKeyVisible((current) => !current)}
          >
            {apiKeyVisible ? <EyeOffIcon size={16} /> : <PreviewIcon size={16} />}
            {apiKeyVisible ? "隐藏密钥" : "查看密钥"}
          </Button>
        </div>
      </div>
      )}
      {!official && (
      <MainModelField
        draft={draft}
        busy={busy}
        baseUrl={baseUrl}
        codex={codex}
        codexSettings={codexSettings}
        claudeSettings={claudeSettings}
        models={models}
        modelsBusy={modelsBusy}
        modelsError={modelsError}
        userConfigModel={userConfigModel}
        userConfigWarnings={userConfigWarnings}
        fetchModels={fetchModels}
        setDraft={setDraft}
      />
      )}

      {official && (
        <fieldset className="asb-fieldset">
          <legend>官方登录</legend>
          <OfficialLoginPanel app={draft.app} onFinished={setLoginDone} />
        </fieldset>
      )}

      {!official && !codex && (
        <ClaudeModelMapping
          busy={busy}
          models={models}
          claudeSettings={claudeSettings}
          setDraft={setDraft}
        />
      )}

      <div className="asb-form-actions">
        <Button variant="secondary" disabled={busy} onClick={onCancel}>
          取消
        </Button>
        {/* A new official profile needs a completed login so it never saves
            credential-free; editing one saves right away — the credentials
            live in the client cache and a profile write never touches them. */}
        <Button
          type="submit"
          variant="primary"
          disabled={busy || (official && !profile && !loginDone)}
        >
          保存供应商
        </Button>
      </div>
    </form>
  );
}
