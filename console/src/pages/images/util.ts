// Constants shared by the image tabs.

import type { ImageType } from '../../api/images.ts';

export const TYPE_LABEL: Record<ImageType, string> = {
  'zone-native': 'Native zone',
  'zone-lx': 'lx zone',
  'vm-raw': 'VM disk',
  'vm-iso': 'ISO',
};
