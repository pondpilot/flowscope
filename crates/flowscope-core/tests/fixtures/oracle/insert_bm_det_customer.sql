-- INSERT: complex with ROW_NUMBER, many JOINs, standard_hash
INSERT INTO /*+ parallel(16)*/ BMRT_META.DIM_CUSTOMER_TMP
  ( id_customer,
    dt_from,
    dt_to,
    code,
    id_customer_type,
    code_customer_type,
    name_customer_type,
    id_class_a,
    code_class_a,
    id_class_b,
    code_class_b,
    id_class_c,
    code_class_c,
    id_class_d,
    code_class_d,
    id_class_e,
    code_class_e,
    id_class_f,
    code_class_f,
    customer_name_short,
    customer_name,
    customer_name_en_short,
    customer_name_en,
    tax_id,
    regn_num,
    ext_id,
    code_class_g,
    dt_registration,
    num_registration,
    note,
    bic,
    swift,
    telex,
    reg_code,
    id_system,
    id_process,
    dttm_insert,
    dttm_update,
    is_deleted,
    hash
   )
  select
    c.id_customer id,
    c.dt_from,
    c.dt_to,
    c.code c_code,
    id_customer_type,
    ct.CL_CODE as code_customer_type,
    ct.name_clvalue as name_customer_type,
    id_class_a, cla.CL_CODE cla_code,
    id_class_b, clb.CL_CODE clb_code,
    id_class_c, clc.CL_CODE clc_code,
    id_class_d, cld.CL_CODE cld_code,
    id_class_e, cle.CL_CODE cle_code,
    id_class_f, clf.CL_CODE clf_code,
    customer_name_short,
    customer_name,
    customer_name_en_short,
    customer_name_en,
    tax_id,
    regn_num,
    ext_id,
    code_class_g,
    dt_registration,
    num_registration,
    c.note,
    bank.bic, bank.swift, bank.telex, bank.reg_code,
    c.id_system,
    p_id_process,
    sysdate as dttm_insert,
    sysdate as dttm_update,
    c.is_deleted,
    standard_hash(
                  to_char(c.DT_TO,'dd.mm.yyyy hh24:mi:ss')
                  ||'|'||
                  c.CODE
                  ||'|'||
                  to_char(c.ID_CUSTOMER_TYPE)
                  ||'|'||
                  ct.CL_CODE
                  ||'|'||
                  ct.NAME_CLVALUE
                  ||'|'||
                  to_char(c.ID_CLASS_A)
                  ||'|'||
                  cla.CL_CODE
                  ||'|'||
                  to_char(c.ID_CLASS_B)
                  ||'|'||
                  clb.CL_CODE
                  ||'|'||
                  to_char(c.ID_CLASS_C)
                  ||'|'||
                  clc.CL_CODE
                  ||'|'||
                  to_char(c.ID_CLASS_D)
                  ||'|'||
                  cld.CL_CODE
                  ||'|'||
                  to_char(c.ID_CLASS_E)
                  ||'|'||
                  cle.CL_CODE
                  ||'|'||
                  to_char(c.ID_CLASS_F)
                  ||'|'||
                  clf.CL_CODE
                  ||'|'||
                  c.CUSTOMER_NAME_SHORT
                  ||'|'||
                  c.CUSTOMER_NAME
                  ||'|'||
                  c.CUSTOMER_NAME_EN_SHORT
                  ||'|'||
                  c.CUSTOMER_NAME_EN
                  ||'|'||
                  c.TAX_ID
                  ||'|'||
                  c.REGN_NUM
                  ||'|'||
                  c.EXT_ID
                  ||'|'||
                  c.CODE_CLASS_G
                  ||'|'||
                  to_char(c.DT_REGISTRATION,'dd.mm.yyyy hh24:mi:ss')
                  ||'|'||
                  c.NUM_REGISTRATION
                  ||'|'||
                  c.NOTE
                  ||'|'||
                  bank.BIC
                  ||'|'||
                  bank.SWIFT
                  ||'|'||
                  bank.TELEX
                  ||'|'||
                  bank.REG_CODE
                  ||'|'||
                  to_char(c.ID_SYSTEM)
                  )
  from
  (select * from
  (select id_customer,
          CASE WHEN DT_OPEN <> util_pkg.c_dt_min THEN DT_OPEN ELSE DT_FROM2 END DT_FROM,
          CASE WHEN DT_OPEN <> util_pkg.c_dt_min THEN DT_CLOSE ELSE DT_TO END DT_TO,
          ROW_NUMBER () OVER(PARTITION BY id_customer, CASE WHEN DT_OPEN <> util_pkg.c_dt_min THEN DT_OPEN ELSE DT_FROM END ORDER BY DT_FROM DESC) RN,
          dt_open, dt_close, code, id_customer_type, id_class_a, id_class_b, id_class_c, id_class_d, id_class_e, id_class_f, customer_name_short, customer_name, customer_name_en_short, customer_name_en, tax_id, regn_num, ext_id, code_class_g, dt_registration, num_registration, note, id_system, is_deleted
    from core.det_customer
   where is_deleted=0
   ) where RN = 1) c
          join core.reg_subject s     on c.id_customer=s.id_subject and s.is_deleted=0
     left join bmrt.dim_class_a cla on c.id_class_a = cla.id_clvalue
     left join bmrt.dim_class_b clb on c.id_class_b = clb.id_clvalue
     left join bmrt.dim_class_c clc on c.id_class_c = clc.id_clvalue
     left join bmrt.dim_class_d cld on c.id_class_d = cld.id_clvalue
     left join bmrt.dim_class_e cle on c.id_class_e = cle.id_clvalue
     left join bmrt.dim_class_f clf on c.id_class_f = clf.id_clvalue
     left join core.det_bank    bank on c.id_customer=bank.id_bank and bank.dt_to=util_pkg.c_dt_max and bank.is_deleted=0
     left join bmrt.dim_customer_type ct on c.id_customer_type=ct.id_clvalue;
