// SPDX-License-Identifier: AGPL-3.0-or-later
import type { Auth } from "./api";
import { useTranslation } from "./i18n";
import { text } from "./i18n/messages";
import { LanguageSwitcher } from "./LanguageSwitcher";

/** The navigation column: views, device count, language and pairing state. */
export function Sidebar({
  workspace,
  deviceCount,
  auth,
  onNavigate,
  onShowDevices,
}: {
  workspace: string;
  deviceCount: number;
  auth: Auth | null;
  onNavigate: (view: string) => void;
  onShowDevices: () => void;
}) {
  const { t } = useTranslation();
  return (
    <aside className="sidebar">
      <a className="brand" href="/">
        CAN<span>Harbor</span>
        <small>{text("main:local_workbench")}</small>
      </a>
      <div className="stage">{text("main:local_can_workbench")}</div>
      <nav>
        <a
          href="#live"
          className={workspace === "live" ? "active" : ""}
          onClick={(e) => {
            e.preventDefault();
            onNavigate("live");
          }}
        >
          {text("main:workspace")} <span>01</span>
        </a>
        <a href="#devices" onClick={onShowDevices}>
          {text("main:devices")} <span>{deviceCount}</span>
        </a>
        <a
          href="#capture"
          className={workspace === "capture" ? "active" : ""}
          onClick={(e) => {
            e.preventDefault();
            onNavigate("capture");
          }}
        >
          {t("recordings")}
        </a>
        <a
          href="#offline"
          className={workspace === "offline" ? "active" : ""}
          onClick={(e) => {
            e.preventDefault();
            onNavigate("offline");
          }}
        >
          {t("offline")}
        </a>
        <a
          href="#stress"
          className={workspace === "stress" ? "active" : ""}
          onClick={(e) => {
            e.preventDefault();
            onNavigate("stress");
          }}
        >
          {t("stress")}
        </a>
        <a href="#about">{t("about")}</a>
      </nav>
      <LanguageSwitcher auth={auth} />
      <div className="side-footer">
        <span className={`dot ${auth ? "connected" : ""}`} />
        <span>
          {auth ? text("main:browser_paired") : text("main:pairing_required")}
        </span>
        <p>{text("main:local_access_no_cloud_connection")}</p>
      </div>
    </aside>
  );
}
