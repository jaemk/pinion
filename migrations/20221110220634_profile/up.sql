begin;

create table pin.profiles
(
    id       bigint primary key   default pin.id_gen(),
    user_id  bigint not null references pin.users(id),
    name     text,
    deleted  boolean     not null default false,
    created  timestamptz not null default now(),
    modified timestamptz not null default now()
);
create index idx_profiles_user_id on pin.profiles(user_id);

commit;