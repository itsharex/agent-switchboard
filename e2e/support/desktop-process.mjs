import { execFile } from 'node:child_process';
import { promisify } from 'node:util';

const exec = promisify(execFile);

async function desktopProcesses(executable) {
  const script = 'Get-CimInstance Win32_Process | Where-Object { $_.ExecutablePath -eq $env:ASB_E2E_BINARY } | Select-Object ProcessId,ExecutablePath | ConvertTo-Json -Compress';
  const { stdout } = await exec('powershell.exe', ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', script], {
    env: { ...process.env, ASB_E2E_BINARY: executable }, windowsHide: true, timeout: 20000,
  });
  return stdout.trim() ? [JSON.parse(stdout)].flat() : [];
}

export async function assertNoDesktop(executable) {
  const processes = await desktopProcesses(executable);
  if (processes.length) throw new Error(`An E2E app is already running (${processes.map((p) => p.ProcessId)}); refusing to share its state`);
}

export async function stopDesktop(executable) {
  for (const entry of await desktopProcesses(executable)) {
    const script = '$p = Get-CimInstance Win32_Process -Filter ("ProcessId = " + $env:ASB_E2E_PID); if ($p -and $p.ExecutablePath -eq $env:ASB_E2E_BINARY) { Stop-Process -Id $p.ProcessId -Force -ErrorAction Stop }';
    await exec('powershell.exe', ['-NoLogo', '-NoProfile', '-NonInteractive', '-Command', script], {
      env: { ...process.env, ASB_E2E_BINARY: executable, ASB_E2E_PID: String(entry.ProcessId) }, windowsHide: true, timeout: 20000,
    });
  }
  await assertNoDesktop(executable);
}
