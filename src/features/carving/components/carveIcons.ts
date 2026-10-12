import type { Component } from 'vue';
import { HardDrive, Layers, Grid2X2, FileImage, FileText, FileArchive, Database, FileAudio, FileVideo } from 'lucide-vue-next';
import type { CarveCategory, CarveScope } from '../../../lib/ipc/carve';
export const scopeOptions: { value: CarveScope; label: string; icon: Component }[] = [
  { value: 'whole', label: '整个镜像', icon: HardDrive },
  { value: 'partition', label: '指定分区', icon: Layers },
  { value: 'unallocated', label: '分区间隙', icon: Grid2X2 },
];
export function categoryIcon(category: CarveCategory): Component {
  return {
    image: FileImage,
    document: FileText,
    archive: FileArchive,
    database: Database,
    audio: FileAudio,
    video: FileVideo,
  }[category];
}
