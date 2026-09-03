// Validation and formatting shared by the VM wizard and the detail page.

import type { Bootrom, Vm } from '../../api/vms.ts';
import { parseSize } from '../storage/util.ts';

export const VM_NAME = /^[a-zA-Z0-9][a-zA-Z0-9_.-]{0,62}$/;

export const MIN_MEMORY = 128 * 1024 * 1024;
export const MIN_DISK = 1024 * 1024;

export const BOOTROMS: { value: Bootrom; label: string }[] = [
  { value: 'uefi', label: 'UEFI' },
  { value: 'uefi-csm', label: 'UEFI with CSM (legacy BIOS)' },
];

/** Errors for the sizing fields; empty when they are fine. */
export function sizingErrors(vcpus: string, memory: string): string[] {
  const errors: string[] = [];
  const n = Number(vcpus);
  if (!Number.isInteger(n) || n < 1 || n > 128)
    errors.push('vCPUs must be a whole number, 1 to 128');
  const bytes = parseSize(memory);
  if (bytes === undefined || bytes < MIN_MEMORY)
    errors.push('Memory must be a size of at least 128M');
  return errors;
}

/** A disk size field: bytes when valid. */
export function diskSize(text: string): number | undefined {
  const bytes = parseSize(text);
  return bytes !== undefined && bytes >= MIN_DISK ? bytes : undefined;
}

export function canStart(vm: Vm): boolean {
  return vm.state === 'installed' || vm.state === 'down';
}

/** Whether the VM's disks and media may change now. */
export function isStopped(vm: Vm): boolean {
  return vm.state !== 'running' && vm.state !== 'ready' && vm.state !== 'shutting_down';
}
