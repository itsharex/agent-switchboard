import { expect, it } from "vitest";
import { screen, waitFor, within } from "@testing-library/react";
import userEvent from "@testing-library/user-event";

import {
  exportExtensionPortableMock,
  importExtensionPortableMock,
  renderPage,
  openMcpDetail,
} from "../../test/extensions-page";

it("imports a portable package and reports missing credential slots", async () => {
  importExtensionPortableMock.mockResolvedValue({
    definition: { id: "ext-imported", name: "docs", revision: 1 },
    missingEnvSlots: ["TOKEN"],
    warnings: [],
  });
  const user = userEvent.setup();
  renderPage();

  await user.click(screen.getByRole("button", { name: "更多扩展操作" }));
  await user.click(await screen.findByRole("menuitem", { name: "导入便携包" }));
  const form = screen.getByRole("form", { name: "导入便携包" });
  await user.type(within(form).getByLabelText(/便携包文件路径/), "D:\\pkgs\\docs.json");
  await user.click(within(form).getByRole("button", { name: "导入便携包" }));

  await waitFor(() => expect(importExtensionPortableMock).toHaveBeenCalledWith("D:\\pkgs\\docs.json"));
  // The import finished and the form closed with the definition selected.
  await waitFor(() => expect(screen.queryByRole("form", { name: "导入便携包" })).not.toBeInTheDocument());
});

it("exports a portable package from the detail view with a chosen path", async () => {
  exportExtensionPortableMock.mockResolvedValue(undefined);
  const user = userEvent.setup();
  renderPage();
  const detail = await openMcpDetail(user);

  await user.click(within(detail).getByRole("button", { name: "导出便携包" }));
  const dialog = await screen.findByRole("dialog", { name: "导出便携包 docs" });
  expect(within(dialog).getByText(/不含密钥、服务地址或本机路径/)).toBeInTheDocument();
  await user.type(within(dialog).getByLabelText("导出文件路径"), "D:\\docs-portable.json");
  await user.click(within(dialog).getByRole("button", { name: "导出到文件" }));

  await waitFor(() =>
    expect(exportExtensionPortableMock).toHaveBeenCalledWith("ext-mcp-1", "D:\\docs-portable.json"),
  );
  await waitFor(() =>
    expect(screen.queryByRole("dialog", { name: "导出便携包 docs" })).not.toBeInTheDocument(),
  );
});
