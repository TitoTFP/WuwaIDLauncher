export interface VolumeState {
  value: number;
  savedValue: number;
  muted: boolean;
  mutedBeforePointer: boolean;
}

export interface VolumeView {
  percent: number;
  muted: boolean;
  audioVolume: number;
}

export interface VolumeCommit {
  state: VolumeState;
  changed: boolean;
  view: VolumeView;
}

export type PersistVolume = (volume: number) => void;

export function normalizeVolumePercent(value: number): number {
  if (!Number.isFinite(value)) return 0;
  return Math.max(0, Math.min(100, Math.round(value)));
}

export function createVolumeState(value: number, muted = false): VolumeState {
  const normalized = normalizeVolumePercent(value);
  return {
    value: normalized,
    savedValue: normalized,
    muted,
    mutedBeforePointer: muted,
  };
}

export function syncSavedVolume(
  state: VolumeState,
  value: number,
): VolumeState {
  const normalized = normalizeVolumePercent(value);
  return {
    ...state,
    value: normalized,
    savedValue: normalized,
    mutedBeforePointer: state.muted,
  };
}

export function setVolumeMuted(
  state: VolumeState,
  muted: boolean,
): VolumeState {
  return { ...state, muted };
}

export function beginVolumePointer(state: VolumeState): VolumeState {
  return { ...state, mutedBeforePointer: state.muted };
}

export function previewVolume(state: VolumeState, value: number): VolumeState {
  const normalized = normalizeVolumePercent(value);
  return {
    ...state,
    value: normalized,
    muted: normalized > 0 ? false : state.muted,
  };
}

export function commitVolume(
  state: VolumeState,
  value: number,
  persist?: PersistVolume,
): VolumeCommit {
  const next = previewVolume(state, value);
  const changed = next.value !== state.savedValue;
  const committed = {
    ...next,
    savedValue: next.value,
    mutedBeforePointer: next.muted,
  };
  if (changed) persist?.(committed.value / 100);
  return {
    state: committed,
    changed,
    view: volumeView(committed),
  };
}

export function cancelVolume(state: VolumeState): VolumeState {
  return {
    ...state,
    value: state.savedValue,
    muted: state.mutedBeforePointer,
    mutedBeforePointer: state.mutedBeforePointer,
  };
}

export function volumeView(state: VolumeState): VolumeView {
  return {
    percent: state.value,
    muted: state.muted,
    audioVolume: state.muted ? 0 : state.value / 100,
  };
}
