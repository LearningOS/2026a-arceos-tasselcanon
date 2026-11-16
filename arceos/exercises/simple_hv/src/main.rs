#![cfg_attr(feature = "axstd", no_std)]
#![cfg_attr(feature = "axstd", no_main)]
#![feature(asm_const)]
#![feature(riscv_ext_intrinsics)]

extern crate alloc;
#[cfg(feature = "axstd")]
extern crate axstd as std;
#[macro_use]
extern crate axlog;

mod csrs;
mod loader;
mod regs;
mod sbi;
mod task;
mod vcpu;

use crate::regs::GprIndex::{A0, A1};
use axhal::mem::PhysAddr;
use csrs::defs::hstatus;
use csrs::{RiscvCsrTrait, CSR};
use loader::load_vm_image;
use riscv::register::{scause, sstatus, stval};
use sbi::SbiMessage;
use tock_registers::LocalRegisterCopy;
use vcpu::VmCpuRegisters;
use vcpu::_run_guest;

const VM_ENTRY: usize = 0x8020_0000;

#[cfg_attr(feature = "axstd", no_mangle)]
fn main() {
    ax_println!("Hypervisor ...");

    // A new address space for vm.
    let mut uspace = axmm::new_user_aspace().unwrap();

    // Load vm binary file into address space.
    if let Err(e) = load_vm_image("/sbin/skernel2", &mut uspace) {
        panic!("Cannot load app! {:?}", e);
    }

    // Setup context to prepare to enter guest mode.
    let mut ctx = VmCpuRegisters::default();
    prepare_guest_context(&mut ctx);

    // Setup pagetable for 2nd address mapping.
    let ept_root = uspace.page_table_root();
    prepare_vm_pgtable(ept_root);

    // Kick off vm and wait for it to exit.
    while !run_guest(&mut ctx) {}

    panic!("Hypervisor ok!");
}

fn prepare_vm_pgtable(ept_root: PhysAddr) {
    let hgatp = 8usize << 60 | usize::from(ept_root) >> 12;
    unsafe {
        core::arch::asm!(
            "csrw hgatp, {hgatp}",
            hgatp = in(reg) hgatp,
        );
        core::arch::riscv64::hfence_gvma_all();
    }
}

fn run_guest(ctx: &mut VmCpuRegisters) -> bool {
    let irq_was_enabled = axhal::arch::irqs_enabled();

    axhal::arch::disable_irqs();

    unsafe {
        _run_guest(ctx);
    }
    ctx.trap_csrs.scause = scause::read().bits();
    ctx.trap_csrs.stval = stval::read();
    let finished = vmexit_handler(ctx);

    if irq_was_enabled {
        axhal::arch::enable_irqs();
    }

    finished
}
#[allow(unreachable_code)]
fn vmexit_handler(ctx: &mut VmCpuRegisters) -> bool {
    use scause::{Exception, Interrupt, Trap};

    let scause_bits = ctx.trap_csrs.scause;
    let interrupt_bit = 1usize << (usize::BITS - 1);
    let code = scause_bits & !interrupt_bit;

    let trap_cause = if scause_bits & interrupt_bit != 0 {
        Trap::Interrupt(Interrupt::from(code))
    } else {
        Trap::Exception(Exception::from(code))
    };
    let trap_value = ctx.trap_csrs.stval;
    match trap_cause {
        Trap::Exception(Exception::VirtualSupervisorEnvCall) => {
            let sbi_msg = SbiMessage::from_regs(ctx.guest_regs.gprs.a_regs()).ok();
            ax_println!("VmExit Reason: VSuperEcall: {:?}", sbi_msg);
            if let Some(msg) = sbi_msg {
                match msg {
                    SbiMessage::Reset(_) => {
                        let a0 = ctx.guest_regs.gprs.reg(A0);
                        let a1 = ctx.guest_regs.gprs.reg(A1);
                        ax_println!("a0 = {:#x}, a1 = {:#x}", a0, a1);
                        assert_eq!(a0, 0x6688);
                        assert_eq!(a1, 0x1234);
                        ax_println!("Shutdown vm normally!");
                        return true;
                    }
                    _ => todo!(),
                }
            } else {
                panic!("bad sbi message! ");
            }
        }
        Trap::Exception(Exception::IllegalInstruction) => {
            ax_println!("\x1b[94mTo this line: 104 \x1b[0m");
            if trap_value == 0xf14025f3 {
                ctx.guest_regs.gprs.set_reg(regs::GprIndex::A1, 0x1234);
                ctx.guest_regs.sepc += 4
            } else {
                panic!(
                    "Bad instruction: {:#x} sepc: {:#x}",
                    trap_value, ctx.guest_regs.sepc
                );
            }
        }
        Trap::Exception(Exception::LoadGuestPageFault) => {
            if trap_value == 0x40 && ctx.guest_regs.sepc == VM_ENTRY + 4 {
                ctx.guest_regs.gprs.set_reg(regs::GprIndex::A0, 0x6688);
                ctx.guest_regs.sepc += 4;
            } else {
                panic!(
                    "LoadGuestPageFault: stval{:#x} sepc: {:#x}",
                    trap_value, ctx.guest_regs.sepc
                );
            }
        }
        Trap::Interrupt(Interrupt::SupervisorTimer) => {
            ax_println!("[debug] handling host timer interrupt");
            let handler = match axhal::trap::IRQ.first() {
                Some(&handler) => handler,
                None => panic!("No Interrupt Handler"),
            };
            if let false = handler(scause_bits) {
                panic!("Handle Interrupt Handler Error");
            }
        }
        _ => {
            panic!(
                "Unhandled trap: {:?}, sepc: {:#x}, stval: {:#x}",
                trap_cause, ctx.guest_regs.sepc, trap_value
            );
        }
    }
    false
}

fn prepare_guest_context(ctx: &mut VmCpuRegisters) {
    // Set hstatus
    let mut hstatus =
        LocalRegisterCopy::<usize, hstatus::Register>::new(riscv::register::hstatus::read().bits());
    // Set Guest bit in order to return to guest mode.
    hstatus.modify(hstatus::spv::Guest);
    // Set SPVP bit in order to accessing VS-mode memory from HS-mode.
    hstatus.modify(hstatus::spvp::Supervisor);
    CSR.hstatus.write_value(hstatus.get());
    ctx.guest_regs.hstatus = hstatus.get();

    // Set sstatus in guest mode.
    let mut sstatus = sstatus::read();
    sstatus.set_spp(sstatus::SPP::Supervisor);
    sstatus.set_sie(false);
    ctx.guest_regs.sstatus = sstatus.bits();
    // Return to entry to start vm.
    ctx.guest_regs.sepc = VM_ENTRY;
}
