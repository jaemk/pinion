begin;

create table pin.groups
(
    id               bigint primary key   default pin.id_gen(),
    name             text        not null,
    creating_user_id bigint      not null references pin.users (id),
    deleted          boolean     not null default false,
    created          timestamptz not null default now(),
    modified         timestamptz not null default now()
);
create index idx_groups_name on pin.groups (name)
    where deleted is false;
create index idx_groups_creator on pin.groups (creating_user_id)
    where deleted is false;

create table pin.group_roles
(
    role text primary key
);
insert into pin.group_roles (role)
values ('admin'),
       ('member');

create table pin.group_associations
(
    id        bigint primary key   default pin.id_gen(),
    user_id   bigint      not null references pin.users (id),
    group_id  bigint      not null references pin.groups (id),
    role      text        not null references pin.group_roles (role),
    sort_rank bigint,
    deleted   boolean     not null default false,
    created   timestamptz not null default now(),
    modified  timestamptz not null default now()
);
create index idx_group_associations_user on pin.group_associations (user_id)
    where deleted is false;
create index idx_group_associations_group on pin.group_associations (group_id)
    where deleted is false;
create unique index idx_group_associations_user_group on pin.group_associations (user_id, group_id)
    where deleted is false;
create index idx_group_associations_created on pin.group_associations (created)
    where deleted is false;

commit;