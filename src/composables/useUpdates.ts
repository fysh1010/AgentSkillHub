import { ref } from "vue";
import * as api from "../api/maintenance";

const info = ref<api.UpdateInfo>();
const busy = ref(false);
const error = ref("");
const automatic = ref(true);
const dismissed = ref(false);

export function useUpdates() {
  async function check(force = false) {
    if (busy.value) return;
    busy.value = true; error.value = "";
    try { info.value = await api.checkAppUpdate(force); dismissed.value = false; }
    catch (e) { error.value = String(e); }
    finally { busy.value = false; }
  }
  async function initialize() {
    try {
      automatic.value = (await api.updateSettings()).check_on_start;
      if (automatic.value) await check();
    } catch (e) { error.value = String(e); }
  }
  async function setAutomatic(value: boolean) {
    await api.saveUpdateSettings({ check_on_start: value });
    automatic.value = value;
  }
  return { info, busy, error, automatic, dismissed, check, initialize, setAutomatic, open: api.openUpdateLink };
}
