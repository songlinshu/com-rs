use proc_macro2::{Ident, TokenStream as HelperTokenStream};
use quote::{format_ident, quote};
use std::collections::HashMap;
use syn::ItemStruct;

fn get_iclass_factory_interface_ident() -> Ident {
    format_ident!("IClassFactory")
}

pub fn get_class_factory_base_interface_idents() -> Vec<Ident> {
    vec![get_iclass_factory_interface_ident()]
}

pub fn get_class_factory_aggr_map() -> HashMap<Ident, Vec<Ident>> {
    HashMap::new()
}

// We manually generate a ClassFactory without macros, otherwise
// it leads to an infinite loop.
pub fn generate(struct_item: &ItemStruct) -> HelperTokenStream {
    // Manually define base_interface_idents and aggr_map usually obtained by
    // parsing attributes.

    let base_interface_idents = get_class_factory_base_interface_idents();
    let aggr_map = get_class_factory_aggr_map();

    let struct_ident = &struct_item.ident;
    let class_factory_ident = macro_utils::class_factory_ident(&struct_ident);

    let struct_definition = gen_class_factory_struct_definition(&class_factory_ident);
    let lock_server = gen_lock_server();
    let iunknown_impl = gen_iunknown_impl(&base_interface_idents, &aggr_map, &class_factory_ident);
    let class_factory_impl = gen_class_factory_impl(&base_interface_idents, &class_factory_ident);

    quote! {
        #struct_definition

        impl com::IClassFactory for #class_factory_ident {
            unsafe fn create_instance(
                &self,
                aggr: *mut <dyn com::IUnknown as com::ComInterface>::VPtr,
                riid: winapi::shared::guiddef::REFIID,
                ppv: *mut *mut winapi::ctypes::c_void,
            ) -> winapi::shared::winerror::HRESULT {
                // Bringing trait into scope to access IUnknown methods.
                use com::IUnknown;

                println!("Creating instance for {}", stringify!(#struct_ident));
                if aggr != std::ptr::null_mut() {
                    return winapi::shared::winerror::CLASS_E_NOAGGREGATION;
                }

                let mut instance = #struct_ident::new();
                instance.add_ref();
                let hr = instance.query_interface(riid, ppv);
                instance.release();

                core::mem::forget(instance);
                hr
            }

            #lock_server
        }

        #iunknown_impl

        #class_factory_impl
    }
}

// Can't use gen_base_fields here, since user might not have imported com::IClassFactory.
pub fn gen_class_factory_struct_definition(class_factory_ident: &Ident) -> HelperTokenStream {
    let ref_count_field = crate::com_struct::gen_ref_count_field();
    let interface_ident = get_iclass_factory_interface_ident();
    let vptr_field_ident = macro_utils::vptr_field_ident(&interface_ident);
    quote! {
        #[repr(C)]
        pub struct #class_factory_ident {
            #vptr_field_ident: <dyn com::IClassFactory as com::ComInterface>::VPtr,
            #ref_count_field
        }
    }
}

pub fn gen_lock_server() -> HelperTokenStream {
    quote! {
        // TODO: Implement correctly
        fn lock_server(&self, _increment: winapi::shared::minwindef::BOOL) -> winapi::shared::winerror::HRESULT {
            println!("LockServer called");
            winapi::shared::winerror::S_OK
        }
    }
}

pub fn gen_iunknown_impl(
    base_interface_idents: &[Ident],
    aggr_map: &HashMap<Ident, Vec<Ident>>,
    class_factory_ident: &Ident,
) -> HelperTokenStream {
    let query_interface = gen_query_interface(class_factory_ident);
    let add_ref = crate::iunknown_impl::gen_add_ref();
    let release = gen_release(&base_interface_idents, &aggr_map, class_factory_ident);
    quote! {
        impl com::IUnknown for #class_factory_ident {
            #query_interface
            #add_ref
            #release
        }
    }
}

pub fn gen_release(
    base_interface_idents: &[Ident],
    aggr_map: &HashMap<Ident, Vec<Ident>>,
    struct_ident: &Ident,
) -> HelperTokenStream {
    let ref_count_ident = macro_utils::ref_count_ident();

    let release_decrement = crate::iunknown_impl::gen_release_decrement(&ref_count_ident);
    let release_assign_new_count_to_var = crate::iunknown_impl::gen_release_assign_new_count_to_var(
        &ref_count_ident,
        &ref_count_ident,
    );
    let release_new_count_var_zero_check =
        crate::iunknown_impl::gen_new_count_var_zero_check(&ref_count_ident);
    let release_drops =
        crate::iunknown_impl::gen_release_drops(base_interface_idents, aggr_map, struct_ident);

    quote! {
        unsafe fn release(&self) -> u32 {
            use com::IClassFactory;

            #release_decrement
            #release_assign_new_count_to_var
            if #release_new_count_var_zero_check {
                #release_drops
            }

            #ref_count_ident
        }
    }
}

fn gen_query_interface(class_factory_ident: &Ident) -> HelperTokenStream {
    let vptr_field_ident = macro_utils::vptr_field_ident(&get_iclass_factory_interface_ident());

    quote! {
        unsafe fn query_interface(&self, riid: *const winapi::shared::guiddef::IID, ppv: *mut *mut winapi::ctypes::c_void) -> winapi::shared::winerror::HRESULT {
            // Bringing trait into scope to access add_ref method.
            use com::IUnknown;

            println!("Querying interface on {}...", stringify!(#class_factory_ident));

            let riid = &*riid;
            if winapi::shared::guiddef::IsEqualGUID(riid, &<dyn com::IUnknown as com::ComInterface>::IID) | winapi::shared::guiddef::IsEqualGUID(riid, &<dyn com::IClassFactory as com::ComInterface>::IID) {
                *ppv = &self.#vptr_field_ident as *const _ as *mut winapi::ctypes::c_void;
                self.add_ref();
                winapi::shared::winerror::NOERROR
            } else {
                *ppv = std::ptr::null_mut::<winapi::ctypes::c_void>();
                winapi::shared::winerror::E_NOINTERFACE
            }
        }
    }
}

pub fn gen_class_factory_impl(
    base_interface_idents: &[Ident],
    class_factory_ident: &Ident,
) -> HelperTokenStream {
    let ref_count_field = crate::com_struct_impl::gen_allocate_ref_count_field();
    let base_fields = crate::com_struct_impl::gen_allocate_base_fields(base_interface_idents);
    let base_inits =
        crate::com_struct_impl::gen_allocate_base_inits(class_factory_ident, base_interface_idents);

    quote! {
        impl #class_factory_ident {
            pub(crate) fn new() -> Box<#class_factory_ident> {
                use com::IClassFactory;

                // allocate directly since no macros generated an `allocate` function
                println!("Allocating new Vtable for {}...", stringify!(#class_factory_ident));
                #base_inits

                let out = #class_factory_ident {
                    #base_fields
                    #ref_count_field
                };
                Box::new(out)
            }
        }
    }
}
