import { computed, watch, type Ref } from 'vue';
import { usePreviewDock } from '../../preview/api';
import { broadcastPreview, onPreviewBroadcast } from '../../preview/api';
import type { FsTypeIdent, StoredDigest } from '../../../lib/ipc/fs/derived';
import type { DiskFsNode } from '../model/model';
import type { FileByteFocus } from '../model/navigation';
import type { FilePreviewReadingState } from '../../../lib/models/filePreviewReadingState';

/** Connects the file panel selection to its inline/detached preview owner. */
export function useFilePreviewBridge(options: {
  imageId: () => string;
  indexOnly?: () => boolean;
  activate: (imageId: string) => boolean;
  node: Ref<DiskFsNode | null>;
  focusTarget: Ref<FileByteFocus | null>;
  visible: Ref<boolean>;
  fullPath: (node: DiskFsNode) => string;
  digest: (nodeId: string) => StoredDigest | undefined;
  ident: (nodeId: string) => FsTypeIdent | undefined;
  identified: (imageId: string, nodeId: string, ident: FsTypeIdent) => void;
  readingState: Ref<FilePreviewReadingState | undefined>;
  snapshotReadingState: () => FilePreviewReadingState | undefined;
}) {
  const { node: detailNode, focusTarget: detailFocus, visible: previewVisible } = options;
  watch(detailNode, node => { if (node?.id !== detailFocus.value?.nodeId) detailFocus.value = null; });
  watch(detailNode, node => { if (node) previewVisible.value = true; });
  const { opening, detached, detach, sync, focus } = usePreviewDock<DiskFsNode>({
    imageId: options.imageId,
    activate: options.activate,
    closed: () => { previewVisible.value = false; },
    restore: (node, focusTarget, readingState) => {
      options.readingState.value = readingState;
      detailNode.value = node;
      detailFocus.value = focusTarget ? { nodeId: node.id, ...focusTarget } : null;
      previewVisible.value = true;
    },
  });
  const dockedPreviewVisible = computed(() => previewVisible.value && !detached.value);
  function togglePreview() {
    if (detached.value) void focus();
    else previewVisible.value = !previewVisible.value;
  }
  function selectionOf(node: DiskFsNode) {
    const fullPath = options.fullPath(node);
    const context = options.snapshotReadingState();
    return {
      indexOnly: options.indexOnly?.(),
      imageId: options.imageId(), nodeId: node.id, title: node.name, fullPath,
      node: { ...node, path: fullPath },
      focusTarget: detailFocus.value?.nodeId === node.id ? detailFocus.value : undefined,
      ident: options.ident(node.id), digest: options.digest(node.id),
      readingState: context?.imageId === options.imageId() && context.nodeId === node.id ? context : undefined,
    };
  }
  async function detachPreview(position?: { x: number; y: number }) {
    const node = detailNode.value;
    if (!node) return;
    const selection = selectionOf(node);
    const opened = await detach({ ...selection, position });
    if (opened && options.imageId() === selection.imageId && detailNode.value === node) previewVisible.value = false;
  }
  watch(() => {
    const node = detailNode.value;
    return [node, detailFocus.value, options.imageId(), options.indexOnly?.(), node && options.ident(node.id), node && options.digest(node.id)];
  }, () => {
    if (detailNode.value) void sync(selectionOf(detailNode.value));
  }, { flush: 'post' });

  function onDetailIdentified(payload: { nodeId: string; ident: FsTypeIdent }) {
    const imageId = options.imageId();
    options.identified(imageId, payload.nodeId, payload.ident);
    broadcastPreview({ kind: 'ident', imageId, ...payload });
  }
  onPreviewBroadcast(message => {
    if (message.imageId === options.imageId() && message.kind === 'ident') options.identified(message.imageId, message.nodeId, message.ident);
  });
  return { previewOpening: opening, previewDetached: detached, dockedPreviewVisible, togglePreview, detachPreview, onDetailIdentified };
}
