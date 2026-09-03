// Typed facade over the design-system components (ADR-0008).
//
// The export ships JSX components and, separately, `*Props` interfaces.
// This file binds the two so pages import typed components from one place
// and nothing under src/ touches design/components directly.

import type {
  ButtonHTMLAttributes,
  ComponentType,
  CSSProperties,
  HTMLAttributes,
  InputHTMLAttributes,
  ReactElement,
  ReactNode,
  SelectHTMLAttributes,
} from 'react';

import type { AlertProps } from '../../design/types/alert/Alert';
import type { ButtonGroupProps, ButtonProps } from '../../design/types/button/Button';
import type { CardProps } from '../../design/types/card/Card';
import type { DatagridProps } from '../../design/types/datagrid/Datagrid';
import type { DropdownProps } from '../../design/types/dropdown/Dropdown';
import type { CheckboxProps } from '../../design/types/forms/Checkbox';
import type { FormFieldProps } from '../../design/types/forms/FormField';
import type { InputProps } from '../../design/types/forms/Input';
import type { PasswordProps } from '../../design/types/forms/Password';
import type { SelectProps } from '../../design/types/forms/Select';
import type { HeaderProps } from '../../design/types/header/Header';
import type { IconProps } from '../../design/types/icon/Icon';
import type { LabelProps } from '../../design/types/label-badge/Label';
import type { ModalProps } from '../../design/types/modal/Modal';
import type { ProgressBarProps } from '../../design/types/progress/ProgressBar';
import type { StackViewProps } from '../../design/types/stack-view/StackView';
import type { TableProps } from '../../design/types/table/Table';
import type { TabsProps } from '../../design/types/tabs/Tabs';
import type { VerticalNavProps } from '../../design/types/vertical-nav/VerticalNav';
import type { WizardProps } from '../../design/types/wizard/Wizard';

import * as alert from '../../design/components/alert/Alert.jsx';
import * as button from '../../design/components/button/Button.jsx';
import * as card from '../../design/components/card/Card.jsx';
import * as datagrid from '../../design/components/datagrid/Datagrid.jsx';
import * as dropdown from '../../design/components/dropdown/Dropdown.jsx';
import * as checkbox from '../../design/components/forms/Checkbox.jsx';
import * as formField from '../../design/components/forms/FormField.jsx';
import * as input from '../../design/components/forms/Input.jsx';
import * as password from '../../design/components/forms/Password.jsx';
import * as select from '../../design/components/forms/Select.jsx';
import * as header from '../../design/components/header/Header.jsx';
import * as icon from '../../design/components/icon/Icon.jsx';
import * as label from '../../design/components/label-badge/Label.jsx';
import * as modal from '../../design/components/modal/Modal.jsx';
import * as progress from '../../design/components/progress/ProgressBar.jsx';
import * as stackView from '../../design/components/stack-view/StackView.jsx';
import * as table from '../../design/components/table/Table.jsx';
import * as tabs from '../../design/components/tabs/Tabs.jsx';
import * as verticalNav from '../../design/components/vertical-nav/VerticalNav.jsx';
import * as wizard from '../../design/components/wizard/Wizard.jsx';

function bind<P>(component: unknown): ComponentType<P> {
  return component as ComponentType<P>;
}

type Status = 'info' | 'success' | 'warning' | 'danger';
interface WithClass {
  className?: string;
}

// Header family. HeaderProps omits the runtime-only `brand` and `className`.
export const Header = bind<HeaderProps & WithClass & { brand?: boolean }>(header.Header);
export const HeaderDivider = bind<Record<string, never>>(header.HeaderDivider);
export const HeaderDropdown = bind<{
  label?: string;
  value?: string;
  items?: string[];
  onSelect?: (item: string) => void;
}>(header.HeaderDropdown);
export const HeaderAction = bind<{
  icon: string;
  badge?: number | string;
  label?: string;
  onClick?: () => void;
}>(header.HeaderAction);
export interface SubnavItem {
  label: string;
  href?: string;
  active?: boolean;
  name?: string;
}
export const Subnav = bind<{
  items?: SubnavItem[];
  onNavigate?: (item: SubnavItem) => void;
  className?: string;
}>(header.Subnav);

export type NavItem = VerticalNavProps['groups'][number]['items'][number];
export type NavGroup = VerticalNavProps['groups'][number];
export const VerticalNav = bind<
  Omit<VerticalNavProps, 'onNavigate'> & {
    onNavigate?: (item: NavItem) => void;
    className?: string;
  }
>(verticalNav.VerticalNav);

export const Button = bind<
  ButtonProps & Omit<ButtonHTMLAttributes<HTMLButtonElement>, keyof ButtonProps>
>(button.Button);
export const ButtonGroup = bind<ButtonGroupProps>(button.ButtonGroup);

export const FormField = bind<FormFieldProps>(formField.FormField);
export const Input = bind<
  InputProps & Omit<InputHTMLAttributes<HTMLInputElement>, keyof InputProps>
>(input.Input);
export const Textarea = bind<
  HTMLAttributes<HTMLTextAreaElement> & { rows?: number; value?: string }
>(input.Textarea);
export const Password = bind<
  PasswordProps & Omit<InputHTMLAttributes<HTMLInputElement>, keyof PasswordProps>
>(password.Password);
export const Select = bind<
  SelectProps & Omit<SelectHTMLAttributes<HTMLSelectElement>, keyof SelectProps>
>(select.Select);
export const Checkbox = bind<
  CheckboxProps & Omit<InputHTMLAttributes<HTMLInputElement>, keyof CheckboxProps>
>(checkbox.Checkbox);
export const Toggle = bind<
  { label?: string; labelLeft?: boolean } & InputHTMLAttributes<HTMLInputElement>
>(checkbox.Toggle);

export interface DatagridColumn<Row> {
  key: string;
  label: string;
  sortable?: boolean;
  width?: number | string;
  render?: (row: Row) => ReactNode;
}
export type DatagridRowProps<Row> = Omit<DatagridProps, 'columns' | 'rows'> & {
  columns: DatagridColumn<Row>[];
  rows: Row[];
  className?: string;
};
// Generic over the row type; the upstream interface uses `any`.
export const Datagrid = datagrid.Datagrid as unknown as <Row>(
  props: DatagridRowProps<Row>,
) => ReactElement | null;

export const Table = bind<TableProps>(table.Table);

export const Modal = bind<ModalProps & WithClass>(modal.Modal);
export const SidePanel = bind<{
  open: boolean;
  title?: ReactNode;
  onClose?: () => void;
  footer?: ReactNode;
  children?: ReactNode;
  width?: number | string;
}>(modal.SidePanel);

export const Alert = bind<AlertProps & WithClass>(alert.Alert);

export const Label = bind<LabelProps & WithClass & { style?: CSSProperties }>(label.Label);
export const Badge = bind<
  { status?: Status; accent?: boolean; children: ReactNode } & HTMLAttributes<HTMLSpanElement>
>(label.Badge);

export const Card = bind<CardProps & WithClass & { style?: CSSProperties }>(card.Card);
export const CardBlock = bind<{
  title?: ReactNode;
  text?: ReactNode;
  className?: string;
  children?: ReactNode;
}>(card.CardBlock);

export const ProgressBar = bind<ProgressBarProps>(progress.ProgressBar);
export const Spinner = bind<
  {
    size?: 'sm' | 'md' | 'lg';
    inline?: boolean;
    inverse?: boolean;
  } & HTMLAttributes<HTMLSpanElement>
>(progress.Spinner);
export const Skeleton = bind<{
  width?: number | string;
  height?: number | string;
  className?: string;
}>(progress.Skeleton);

export const Icon = bind<IconProps & WithClass & { style?: CSSProperties }>(icon.Icon);

export type DropdownItem = DropdownProps['items'][number];
export const Dropdown = bind<DropdownProps & WithClass>(dropdown.Dropdown);

export const Tabs = bind<TabsProps & WithClass>(tabs.Tabs);
export const StackView = bind<StackViewProps>(stackView.StackView);

export const Wizard = bind<WizardProps & WithClass>(wizard.Wizard);
